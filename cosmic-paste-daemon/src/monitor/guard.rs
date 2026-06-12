use std::time::{Duration, Instant};

/// Ignores monitor notifications matching a recent daemon-initiated copy.
#[derive(Clone, Debug)]
pub struct SelfCopyGuard {
    window: Duration,
    last_fingerprint: Option<[u8; 32]>,
    last_set: Option<Instant>,
}

impl SelfCopyGuard {
    pub fn new() -> Self {
        Self {
            // Cover clipboard write timeout (2s) plus monitor debounce slack.
            window: Duration::from_millis(2500),
            last_fingerprint: None,
            last_set: None,
        }
    }

    #[allow(dead_code)] // armed before daemon clipboard writes (Select / SelectAtOffset)
    pub fn arm(&mut self, fingerprint: [u8; 32]) {
        self.last_fingerprint = Some(fingerprint);
        self.last_set = Some(Instant::now());
    }

    pub fn should_ignore(&self, fingerprint: [u8; 32]) -> bool {
        let Some(armed) = self.last_fingerprint else {
            return false;
        };
        let Some(set_at) = self.last_set else {
            return false;
        };
        armed == fingerprint && set_at.elapsed() <= self.window
    }
}

impl Default for SelfCopyGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_matching_fingerprint_inside_window() {
        let mut guard = SelfCopyGuard::new();
        let fp = [7u8; 32];
        guard.arm(fp);
        assert!(guard.should_ignore(fp));
    }

    #[test]
    fn does_not_ignore_unarmed_fingerprint() {
        let guard = SelfCopyGuard::new();
        assert!(!guard.should_ignore([1u8; 32]));
    }
}