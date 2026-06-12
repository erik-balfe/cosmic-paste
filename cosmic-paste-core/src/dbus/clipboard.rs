//! Clipboard write-back requests from DBus handlers to the Wayland monitor thread.

use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::Arc;
use std::time::Duration;

use crate::item::text_checksum;

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

pub type ClipboardWriteSender = Arc<std::sync::mpsc::Sender<ClipboardWriteRequest>>;

const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

/// Send a clipboard write and wait for the Wayland thread to complete it.
pub async fn write_clipboard(
    tx: &ClipboardWriteSender,
    text: &str,
) -> zbus::fdo::Result<()> {
    let (request, reply_rx) = ClipboardWriteRequest::new(text.to_owned());
    tx.send(request).map_err(|_| {
        zbus::fdo::Error::Failed("clipboard writer unavailable".into())
    })?;

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