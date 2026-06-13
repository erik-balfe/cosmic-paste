//! Clipboard monitor (ADR-001: wlr-data-control on dedicated thread → tokio mpsc).

mod data_control;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use cosmic_paste_core::dbus::ClipboardWriteRequest;
pub use cosmic_paste_core::dbus::SharedSelfCopyGuard;
use tokio::sync::mpsc as async_mpsc;

/// Which Wayland selection produced the payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionSource {
    Clipboard,
    Primary,
}

/// Normalized clipboard change delivered to the tokio ingest loop.
#[derive(Clone, Debug)]
pub struct ClipboardEvent {
    pub source: SelectionSource,
    pub mime_type: String,
    pub payload: Vec<u8>,
    pub observed_at: u64,
}

#[derive(Clone, Debug)]
pub struct MonitorConfig {
    pub debounce: Duration,
    pub watch_primary: bool,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(75),
            watch_primary: false,
        }
    }
}

/// Handle to the background monitor thread.
pub struct ClipboardMonitor {
    guard: SharedSelfCopyGuard,
    config: Arc<Mutex<MonitorConfig>>,
}

impl ClipboardMonitor {
    pub fn new(config: MonitorConfig, guard: SharedSelfCopyGuard) -> Self {
        Self {
            guard,
            config: Arc::new(Mutex::new(config)),
        }
    }

    pub fn shared_config(&self) -> Arc<Mutex<MonitorConfig>> {
        self.config.clone()
    }

    /// Spawn the monitor thread and return the receiver for ingest.
    pub fn spawn(
        self,
        tx: async_mpsc::Sender<ClipboardEvent>,
        write_rx: mpsc::Receiver<ClipboardWriteRequest>,
    ) -> MonitorHandle {
        let guard = self.guard.clone();
        let config = self.config.clone();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_thread = shutdown.clone();
        let clipboard_tx = tx.clone();
        let join = std::thread::Builder::new()
            .name("cosmic-paste-wayland".into())
            .spawn(move || {
                if let Err(err) = data_control::run(tx, write_rx, config, guard, shutdown_thread)
                {
                    tracing::error!("clipboard monitor exited: {err}");
                }
            })
            .expect("spawn wayland monitor thread");

        MonitorHandle {
            join,
            shutdown,
            clipboard_tx: Some(clipboard_tx),
        }
    }
}

pub struct MonitorHandle {
    join: std::thread::JoinHandle<()>,
    shutdown: Arc<AtomicBool>,
    clipboard_tx: Option<async_mpsc::Sender<ClipboardEvent>>,
}

impl MonitorHandle {
    pub fn shutdown(mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        drop(self.clipboard_tx.take());
        match self.join.join() {
            Ok(()) => tracing::debug!("clipboard monitor thread joined"),
            Err(_) => tracing::warn!("clipboard monitor thread panicked"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_debounce_is_75ms() {
        assert_eq!(MonitorConfig::default().debounce, Duration::from_millis(75));
    }
}