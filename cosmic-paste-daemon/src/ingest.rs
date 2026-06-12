use cosmic_paste_core::dbus::state::SharedDaemonState;
use cosmic_paste_core::IngestOutcome;
use tokio::sync::mpsc;

use crate::monitor::ClipboardEvent;
use crate::signals::DaemonSignal;

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Drain clipboard events into daemon history (text/plain in PR 4).
pub async fn run_ingest_loop(
    mut rx: mpsc::Receiver<ClipboardEvent>,
    state: SharedDaemonState,
    signal_tx: Option<mpsc::Sender<DaemonSignal>>,
) {
    while let Some(event) = rx.recv().await {
        if event.mime_type != "text/plain" {
            tracing::debug!(
                mime = %event.mime_type,
                bytes = event.payload.len(),
                "skipping non-text clipboard event (precedence stub)"
            );
            continue;
        }

        let text = match std::str::from_utf8(&event.payload) {
            Ok(text) => text,
            Err(err) => {
                tracing::warn!("clipboard text/plain is not valid UTF-8: {err}");
                continue;
            }
        };

        let mut guard = state.lock().await;
        if !guard.tracking {
            continue;
        }

        let outcome = guard.session_mut().ingest_text(text, None, unix_now());
        if matches!(outcome, IngestOutcome::RejectedTextSize) {
            tracing::debug!("clipboard text rejected by size policy");
            continue;
        }

        if let Err(err) = guard.persist() {
            tracing::error!("failed to persist clipboard ingest: {err}");
            continue;
        }

        let uuid = guard
            .history()
            .get(0)
            .map(|item| item.uuid.to_string())
            .unwrap_or_default();
        let count = guard.history().len() as u32;
        drop(guard);

        if let Some(tx) = &signal_tx {
            let _ = tx
                .send(DaemonSignal::Update {
                    action: "add",
                    target: uuid,
                    index: 0,
                })
                .await;
            let _ = tx
                .send(DaemonSignal::ActiveIndexChanged { index: 0, count })
                .await;
        }

        tracing::debug!(
            bytes = event.payload.len(),
            source = ?event.source,
            observed_at = event.observed_at,
            "ingested clipboard text"
        );
    }
}

#[cfg(test)]
mod tests {
    use cosmic_paste_core::dbus::state::DaemonState;

    use super::*;
    use crate::monitor::{ClipboardEvent, SelectionSource};

    #[tokio::test]
    async fn ingest_loop_adds_text_to_history() {
        let state = DaemonState::new_in_memory()
            .service(cosmic_paste_core::dbus::lifecycle::LifecycleHandle::detached())
            .shared_state();
        let (tx, rx) = mpsc::channel(4);

        let ingest = tokio::spawn(run_ingest_loop(rx, state.clone(), None));

        tx.send(ClipboardEvent {
            source: SelectionSource::Clipboard,
            mime_type: "text/plain".into(),
            payload: b"from clipboard".to_vec(),
            observed_at: 1,
        })
        .await
        .unwrap();
        drop(tx);

        let _ = ingest.await;

        let guard = state.lock().await;
        assert_eq!(guard.history().len(), 1);
        assert_eq!(
            guard.history().get(0).unwrap().plain_text(),
            Some("from clipboard")
        );
    }

    #[tokio::test]
    async fn ingest_loop_skips_non_text_mime() {
        let state = DaemonState::new_in_memory()
            .service(cosmic_paste_core::dbus::lifecycle::LifecycleHandle::detached())
            .shared_state();
        let (tx, rx) = mpsc::channel(4);

        let ingest = tokio::spawn(run_ingest_loop(rx, state.clone(), None));

        tx.send(ClipboardEvent {
            source: SelectionSource::Clipboard,
            mime_type: "image/png".into(),
            payload: vec![0, 1, 2],
            observed_at: 1,
        })
        .await
        .unwrap();
        drop(tx);

        let _ = ingest.await;

        let guard = state.lock().await;
        assert!(guard.history().is_empty());
    }
}