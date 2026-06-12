//! Clipboard monitor (ADR-001: wlr-data-control on dedicated thread → tokio mpsc).

mod data_control;
mod guard;

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use cosmic_paste_core::dbus::ClipboardWriteRequest;
pub use guard::SelfCopyGuard;
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
    guard: Arc<Mutex<SelfCopyGuard>>,
    config: MonitorConfig,
}

impl ClipboardMonitor {
    pub fn new(config: MonitorConfig) -> Self {
        Self {
            guard: Arc::new(Mutex::new(SelfCopyGuard::new())),
            config,
        }
    }

    /// Spawn the monitor thread and return the receiver for ingest.
    pub fn spawn(
        self,
        tx: async_mpsc::Sender<ClipboardEvent>,
        write_rx: mpsc::Receiver<ClipboardWriteRequest>,
    ) -> MonitorHandle {
        let guard = self.guard.clone();
        let config = self.config;
        let join = std::thread::Builder::new()
            .name("cosmic-paste-wayland".into())
            .spawn(move || {
                if let Err(err) = data_control::run(tx, write_rx, config, guard) {
                    tracing::error!("clipboard monitor exited: {err}");
                }
            })
            .expect("spawn wayland monitor thread");

        MonitorHandle { join }
    }
}

pub struct MonitorHandle {
    join: std::thread::JoinHandle<()>,
}

impl MonitorHandle {
    pub fn join(self) {
        let _ = self.join.join();
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