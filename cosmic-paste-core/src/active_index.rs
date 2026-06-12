use crate::error::{Error, Result};

/// Tracks which history entry is "active" for prev/next navigation and panel status.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveIndexState {
    active_index: usize,
    navigation_wrap: bool,
}

impl ActiveIndexState {
    pub fn new(navigation_wrap: bool) -> Self {
        Self {
            active_index: 0,
            navigation_wrap,
        }
    }

    pub fn active_index(&self) -> usize {
        self.active_index
    }

    pub fn set_navigation_wrap(&mut self, wrap: bool) {
        self.navigation_wrap = wrap;
    }

    pub fn on_external_ingest(&mut self) {
        self.active_index = 0;
    }

    pub fn on_select(&mut self) {
        self.active_index = 0;
    }

    pub fn on_pop(&mut self) {
        self.active_index = 0;
    }

    pub fn on_switch_history(&mut self) {
        self.active_index = 0;
    }

    pub fn on_empty_history(&mut self) {
        self.active_index = 0;
    }

    /// Growing-lines replace keeps the entry at the front; active index stays at 0.
    pub fn on_growing_lines_replace(&mut self) {
        self.active_index = 0;
    }

    /// Preview-only highlight; does not write clipboard.
    pub fn set_active_index(&mut self, index: usize, history_len: usize) -> Result<usize> {
        if history_len == 0 {
            self.active_index = 0;
            return Ok(0);
        }

        if index >= history_len {
            return Err(Error::ActiveIndexOutOfRange {
                index,
                len: history_len,
            });
        }

        self.active_index = index;
        Ok(self.active_index)
    }

    /// Global prev/next shortcuts: offset -1 = newer, +1 = older.
    pub fn select_at_offset(&mut self, history_len: usize, offset: i32) -> Result<usize> {
        if history_len == 0 {
            return Err(Error::EmptyHistory);
        }

        let current = self.active_index as i32;
        let target = if self.navigation_wrap {
            let len = history_len as i32;
            ((current + offset).rem_euclid(len)) as usize
        } else {
            let target = current + offset;
            if target < 0 || target >= history_len as i32 {
                return Err(Error::NavigationBoundary {
                    index: self.active_index,
                    len: history_len,
                });
            }
            target as usize
        };

        self.active_index = target;
        Ok(self.active_index)
    }

    pub fn on_delete(&mut self, deleted_index: usize, history_len_after: usize) {
        if history_len_after == 0 {
            self.active_index = 0;
            return;
        }

        if deleted_index < self.active_index {
            self.active_index -= 1;
        } else if deleted_index == self.active_index {
            self.active_index = self.active_index.min(history_len_after - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_and_select_reset_to_zero() {
        let mut state = ActiveIndexState::default();
        state.set_active_index(3, 10).unwrap();
        state.on_external_ingest();
        assert_eq!(state.active_index(), 0);

        state.set_active_index(2, 10).unwrap();
        state.on_select();
        assert_eq!(state.active_index(), 0);
    }

    #[test]
    fn clamp_at_older_boundary() {
        let mut state = ActiveIndexState::default();
        state.set_active_index(2, 3).unwrap();
        let err = state.select_at_offset(3, 1).unwrap_err();
        assert_eq!(
            err,
            Error::NavigationBoundary {
                index: 2,
                len: 3
            }
        );
    }

    #[test]
    fn clamp_at_newer_boundary() {
        let mut state = ActiveIndexState::default();
        let err = state.select_at_offset(3, -1).unwrap_err();
        assert_eq!(
            err,
            Error::NavigationBoundary {
                index: 0,
                len: 3
            }
        );
    }

    #[test]
    fn wrap_when_enabled() {
        let mut state = ActiveIndexState::new(true);
        state.set_active_index(2, 3).unwrap();
        assert_eq!(state.select_at_offset(3, 1).unwrap(), 0);
        assert_eq!(state.select_at_offset(3, -1).unwrap(), 2);
    }

    #[test]
    fn delete_before_active_shifts_index() {
        let mut state = ActiveIndexState::default();
        state.set_active_index(2, 5).unwrap();
        state.on_delete(1, 4);
        assert_eq!(state.active_index(), 1);
    }

    #[test]
    fn delete_active_clamps_index() {
        let mut state = ActiveIndexState::default();
        state.set_active_index(2, 5).unwrap();
        state.on_delete(2, 4);
        assert_eq!(state.active_index(), 2);
    }
}