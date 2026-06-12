use tokio::sync::watch;

/// Daemon shutdown requests surfaced over DBus (`Reexecute`) or handled in `main`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ShutdownReason {
    #[default]
    None,
    Reexecute,
}

/// Signals the daemon main loop from DBus handlers.
#[derive(Clone, Debug)]
pub struct LifecycleHandle {
    tx: watch::Sender<ShutdownReason>,
}

impl LifecycleHandle {
    pub fn pair() -> (Self, watch::Receiver<ShutdownReason>) {
        let (tx, rx) = watch::channel(ShutdownReason::None);
        (Self { tx }, rx)
    }

    /// Handle detached from a receiver — sufficient for unit tests.
    pub fn detached() -> Self {
        Self::pair().0
    }

    pub fn request(&self, reason: ShutdownReason) {
        let _ = self.tx.send(reason);
    }

    pub fn subscribe(&self) -> watch::Receiver<ShutdownReason> {
        self.tx.subscribe()
    }
}