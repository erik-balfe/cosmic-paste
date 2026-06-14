//! Clipboard write-back requests from DBus handlers to the Wayland monitor thread.

use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::Duration;

use crate::item::text_checksum;

use super::guard::SharedSelfCopyGuard;

/// Write plain text to the Wayland clipboard selection (wlr-data-control).
pub struct ClipboardWriteRequest {
    pub text: String,
    pub fingerprint: [u8; 32],
    pub reply: SyncSender<Result<(), String>>,
}

impl ClipboardWriteRequest {
    pub fn new(text: String) -> (Self, Receiver<Result<(), String>>) {
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        let fingerprint = text_checksum(&text);
        (
            Self {
                text,
                fingerprint,
                reply: reply_tx,
            },
            reply_rx,
        )
    }
}

pub type ClipboardWriteSender = Arc<SyncSender<ClipboardWriteRequest>>;

pub const WRITE_TIMEOUT: Duration = Duration::from_millis(5000);

pub const WRITE_QUEUE_DEPTH: usize = 8;

fn write_wl_copy_sync(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    if text.is_empty() {
        return Err("refusing to write empty clipboard text".into());
    }

    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to spawn wl-copy: {err}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|err| format!("failed to write wl-copy stdin: {err}"))?;
    }
    let status = child
        .wait()
        .map_err(|err| format!("failed to wait for wl-copy: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("wl-copy exited with {status}"))
    }
}

/// Write via wl-clipboard (reliable for Ctrl+V in COSMIC apps).
pub async fn write_wl_copy(text: &str) -> zbus::fdo::Result<()> {
    let text = text.to_owned();
    tokio::task::spawn_blocking(move || write_wl_copy_sync(&text))
        .await
        .map_err(|_| zbus::fdo::Error::Failed("wl-copy task panicked".into()))?
        .map_err(zbus::fdo::Error::Failed)
}

fn arm_self_copy_guard(guard: Option<&SharedSelfCopyGuard>, text: &str) {
    let Some(guard) = guard else {
        return;
    };
    if let Ok(mut guard) = guard.lock() {
        guard.arm(text_checksum(text));
    }
}

/// Test harness: arm guard without touching the system clipboard.
#[cfg(test)]
pub async fn ack_clipboard_write_for_test(
    guard: Option<&SharedSelfCopyGuard>,
    text: &str,
) -> zbus::fdo::Result<()> {
    if text.is_empty() {
        return Err(zbus::fdo::Error::Failed(
            "selected history item has no pasteable text".into(),
        ));
    }
    arm_self_copy_guard(guard, text);
    Ok(())
}

/// Fast paste path: arm the self-copy guard, then write via wl-copy only.
///
/// A background data-control write used to race wl-copy and could replace the
/// selection with an incomplete source, leaving the clipboard empty.
pub async fn write_clipboard_for_paste(
    guard: Option<&SharedSelfCopyGuard>,
    text: &str,
) -> zbus::fdo::Result<()> {
    if text.is_empty() {
        return Err(zbus::fdo::Error::Failed(
            "selected history item has no pasteable text".into(),
        ));
    }
    arm_self_copy_guard(guard, text);
    write_wl_copy(text).await
}

/// Send a clipboard write and wait for the Wayland thread to complete it.
pub async fn write_clipboard(
    tx: &ClipboardWriteSender,
    guard: Option<&SharedSelfCopyGuard>,
    text: &str,
) -> zbus::fdo::Result<()> {
    if text.is_empty() {
        return Err(zbus::fdo::Error::Failed(
            "refusing to write empty clipboard text".into(),
        ));
    }
    arm_self_copy_guard(guard, text);

    let (request, reply_rx) = ClipboardWriteRequest::new(text.to_owned());
    match tx.try_send(request) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            return Err(zbus::fdo::Error::Failed(
                "clipboard write queue is full".into(),
            ));
        }
        Err(TrySendError::Disconnected(_)) => {
            return Err(zbus::fdo::Error::Failed(
                "clipboard writer unavailable".into(),
            ));
        }
    }

    let result = tokio::task::spawn_blocking(move || reply_rx.recv_timeout(WRITE_TIMEOUT))
        .await
        .map_err(|_| zbus::fdo::Error::Failed("clipboard write task panicked".into()))?;

    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(zbus::fdo::Error::Failed(format!(
            "clipboard write failed: {err}"
        ))),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(zbus::fdo::Error::Failed(
            "clipboard write timed out".into(),
        )),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(zbus::fdo::Error::Failed(
            "clipboard writer disconnected".into(),
        )),
    }
}