use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::clipboard::WRITE_TIMEOUT;

/// Ignores monitor notifications while a daemon-initiated clipboard write is pending.
#[derive(Clone, Debug)]
pub struct SelfCopyGuard {
    window: Duration,
    pending: Option<PendingCopy>,
}

#[derive(Clone, Debug)]
struct PendingCopy {
    fingerprint: [u8; 32],
    armed_at: Instant,
}

impl SelfCopyGuard {
    pub fn new() -> Self {
        Self {
            // Match clipboard write timeout so stale reads cannot land after the guard expires.
            window: WRITE_TIMEOUT + Duration::from_millis(250),
            pending: None,
        }
    }

    pub fn arm(&mut self, fingerprint: [u8; 32]) {
        self.pending = Some(PendingCopy {
            fingerprint,
            armed_at: Instant::now(),
        });
    }

    /// True when ingest should ignore this clipboard payload.
    ///
    /// While a navigation/paste write is pending we suppress *all* clipboard
    /// events until the armed fingerprint lands or the window expires. That
    /// avoids ingesting stale pre-write clipboard text (which used to reset
    /// `active_index` to 0 mid-navigation).
    pub fn should_suppress_ingest(&self, fingerprint: [u8; 32]) -> bool {
        let Some(pending) = &self.pending else {
            return false;
        };
        if pending.armed_at.elapsed() > self.window {
            return false;
        }
        let _ = fingerprint;
        true
    }

    /// Clear the pending write once our selection is observed on the clipboard.
    pub fn clear_if_matched(&mut self, fingerprint: [u8; 32]) {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.fingerprint == fingerprint)
        {
            self.pending = None;
        }
    }

}

impl Default for SelfCopyGuard {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedSelfCopyGuard = Arc<Mutex<SelfCopyGuard>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suppresses_stale_clipboard_until_expected_write_lands() {
        let mut guard = SelfCopyGuard::new();
        let expected = [7u8; 32];
        let stale = [3u8; 32];
        guard.arm(expected);
        assert!(guard.should_suppress_ingest(stale));
        assert!(guard.should_suppress_ingest(expected));
        guard.clear_if_matched(expected);
        assert!(!guard.should_suppress_ingest(stale));
    }

    #[test]
    fn does_not_suppress_when_unarmed() {
        let guard = SelfCopyGuard::new();
        assert!(!guard.should_suppress_ingest([1u8; 32]));
    }
}