//! Active-item status line (`N/count| preview`) for tooltip and navigation toasts.

use crate::item::{format_display_line, format_display_line_middle};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

const PREVIEW_LEN: usize = 72;
/// Fixed width for tooltip progress line (`18/59|`).
const TOOLTIP_PROGRESS_LEN: usize = 10;
/// Toast line 1: `18/59` progress (notification summary field).
const TOAST_PROGRESS_LEN: usize = 10;
/// Toast line 2: middle-truncated preview (notification body field).
const TOAST_PREVIEW_LEN: usize = 80;
/// NBSP — COSMIC strips trailing ASCII spaces from notification text.
const NBSP: char = '\u{00A0}';

fn sanitize_notification_text(text: &str) -> String {
    text.chars()
        .map(|ch| match ch {
            '\n' | '\r' | '\t' => ' ',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect()
}

/// Keep the toast visible while navigating; hide only after this idle period.
const HIDE_DEBOUNCE_MS: u64 = 2500;
/// On cosmic-notifications, Notify `expire_timeout: 0` means never expire.
const NOTIFY_EXPIRE_MS: i32 = 0;
const NOTIFY_APP: &str = "COSMIC Paste";

enum ToastCommand {
    Show { summary: String, body: String },
}

static TOAST_TX: OnceLock<mpsc::Sender<ToastCommand>> = OnceLock::new();

fn toast_worker(rx: mpsc::Receiver<ToastCommand>) {
    let debounce = Duration::from_millis(HIDE_DEBOUNCE_MS);
    let mut notification_id: Option<u32> = None;
    let mut hide_at: Option<Instant> = None;

    loop {
        let wait = hide_at
            .map(|deadline| {
                deadline
                    .saturating_duration_since(Instant::now())
                    .max(Duration::from_millis(50))
            })
            .unwrap_or(Duration::from_secs(3600));

        match rx.recv_timeout(wait) {
            Ok(ToastCommand::Show { summary, body }) => {
                notification_id =
                    replace_selection_notification(&summary, &body, notification_id);
                hide_at = notification_id.map(|_| Instant::now() + debounce);
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(deadline) = hide_at
                    && Instant::now() >= deadline
                {
                    if let Some(id) = notification_id.take() {
                        close_notification(id);
                    }
                    hide_at = None;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn ensure_toast_sender() -> Option<&'static mpsc::Sender<ToastCommand>> {
    if let Some(tx) = TOAST_TX.get() {
        return Some(tx);
    }
    let (tx, rx) = mpsc::channel();
    match thread::Builder::new()
        .name("cosmic-paste-toast".into())
        .spawn(move || toast_worker(rx))
    {
        Ok(_) => {
            let _ = TOAST_TX.set(tx);
            TOAST_TX.get()
        }
        Err(err) => {
            tracing::error!("failed to spawn toast worker: {err}");
            None
        }
    }
}

fn pad_display_line(line: &str, width: usize, pad: char) -> String {
    let char_count = line.chars().count();
    if char_count >= width {
        return line.to_string();
    }
    let mut out = String::with_capacity(width);
    out.push_str(line);
    for _ in char_count..width {
        out.push(pad);
    }
    out
}

fn fixed_toast_preview(preview: &str) -> String {
    let clean = sanitize_notification_text(preview);
    let line = format_display_line_middle(&clean, TOAST_PREVIEW_LEN);
    let mut chars = line.chars();
    let core: String = chars.by_ref().take(TOAST_PREVIEW_LEN).collect();
    pad_display_line(&core, TOAST_PREVIEW_LEN, NBSP)
}

/// Two fixed-width lines for COSMIC: summary = progress, body = middle-truncated preview.
fn format_selection_toast_parts(active_index: u32, count: u32, preview: &str) -> (String, String) {
    let position = active_index.saturating_add(1);
    let summary = pad_display_line(&format!("{position}/{count}"), TOAST_PROGRESS_LEN, NBSP);
    let body = fixed_toast_preview(preview);
    (summary, body)
}

/// One-based index, count, ASCII pipe, single-line preview (panel tooltip).
pub fn format_selection_status(active_index: u32, count: u32, preview: &str) -> String {
    let position = active_index.saturating_add(1);
    let line = pad_display_line(
        &format_display_line(preview, PREVIEW_LEN),
        PREVIEW_LEN,
        ' ',
    );
    let progress = pad_display_line(&format!("{position}/{count}|"), TOOLTIP_PROGRESS_LEN, ' ');
    if count == 0 {
        return format!("{progress}\n{line}");
    }
    format!("{progress}\n{line}")
}

fn close_notification(id: u32) {
    let _ = std::process::Command::new("dbus-send")
        .args([
            "--session",
            "--dest=org.freedesktop.Notifications",
            "--type=method_call",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications.CloseNotification",
            &format!("uint32:{id}"),
        ])
        .output();
}

fn replace_selection_notification(
    summary: &str,
    body: &str,
    replace_id: Option<u32>,
) -> Option<u32> {
    if let Some(id) = replace_id {
        if let Some(new_id) = notify(summary, body, Some(id)) {
            return Some(new_id);
        }
        close_notification(id);
    }
    notify(summary, body, None)
}

fn notify(summary: &str, body: &str, replace_id: Option<u32>) -> Option<u32> {
    let output = std::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "-e",
            "-d",
            "org.freedesktop.Notifications",
            "-o",
            "/org/freedesktop/Notifications",
            "-m",
            "org.freedesktop.Notifications.Notify",
            NOTIFY_APP,
            &replace_id.unwrap_or(0).to_string(),
            "",
            summary,
            body,
            "[]",
            "{'expire-timeout': <int32 0>, 'transient': <int32 0>}",
            &NOTIFY_EXPIRE_MS.to_string(),
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    parse_notify_reply(&output.stdout)
}

fn parse_notify_reply(stdout: &[u8]) -> Option<u32> {
    let text = std::str::from_utf8(stdout).ok()?;
    let marker = "uint32 ";
    let start = text.find(marker)? + marker.len();
    text[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

/// Brief COSMIC/desktop notification after keyboard history navigation.
pub fn show_selection_toast(active_index: u32, count: u32, preview: &str) {
    let Some(tx) = ensure_toast_sender() else {
        return;
    };
    let (summary, body) = format_selection_toast_parts(active_index, count, preview);
    if let Err(err) = tx.send(ToastCommand::Show { summary, body }) {
        tracing::warn!("failed to queue selection toast: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_one_based_progress_with_pipe() {
        let line = format_selection_status(4, 7, "myClipTest");
        assert!(line.contains("5/7|"));
        assert!(line.contains("myClipTest"));
        assert_eq!(line.lines().count(), 2);
    }

    #[test]
    fn toast_parts_are_two_fixed_width_lines() {
        let (summary, body) = format_selection_toast_parts(17, 59, "short");
        let (summary_long, body_long) = format_selection_toast_parts(
            0,
            100,
            "beginning of a very long clipboard entry that definitely exceeds the toast preview width and keeps the ending tail visible",
        );
        assert_eq!(summary.chars().count(), TOAST_PROGRESS_LEN);
        assert_eq!(body.chars().count(), TOAST_PREVIEW_LEN);
        assert_eq!(summary_long.chars().count(), TOAST_PROGRESS_LEN);
        assert_eq!(body_long.chars().count(), TOAST_PREVIEW_LEN);
        assert!(!summary.contains('\n'));
        assert!(!body.contains('\n'));
        assert!(body_long.contains('…'));
    }

    #[test]
    fn sanitizes_control_characters_in_toast_preview() {
        let (_, body) = format_selection_toast_parts(0, 3, "line1\nline2");
        assert!(!body.contains('\n'));
    }

    #[test]
    fn parses_gdbus_notify_reply() {
        assert_eq!(parse_notify_reply(b"(uint32 7,)\n"), Some(7));
    }
}