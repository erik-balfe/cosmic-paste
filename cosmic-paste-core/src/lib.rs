//! Core types and in-memory history logic for [cosmic-paste](https://github.com/pop-os/cosmic-epoch).
//!
//! Clipboard monitoring, DBus, and UI live in separate crates. This library owns
//! history items, eviction policies, and the active-index state machine from
//! `docs/DESIGN.md` §1b.

pub mod active_index;
pub mod dbus;
pub mod error;
pub mod history;
pub mod item;
pub mod persistence;
pub mod selection_status;
pub mod settings;
pub mod show_history_trigger;

pub use active_index::ActiveIndexState;
pub use error::{Error, Result};
pub use history::{History, HistoryPolicies, IngestOutcome};
pub use item::{
    collapse_display_text, format_display_line, format_display_line_middle, truncate_display,
    truncate_display_middle, HistoryItem, ItemKind, RichPayload,
};
pub use dbus::{
    client::CosmicPasteProxy, BUS_NAME, INTERFACE_NAME, OBJECT_PATH, VERSION as DAEMON_VERSION,
};
pub use selection_status::{format_selection_status, show_selection_toast};
pub use settings::{Settings, ShortcutSettings, APP_ID as SETTINGS_APP_ID};
pub use persistence::{
    checksum_hex, DataPaths, HistoryFile, HistoryStore, LoadHistoryOutcome, PersistenceError,
    SessionState, FORMAT_MAGIC, FORMAT_VERSION,
};

/// Coordinates history mutations with active-index transitions.
#[derive(Clone, Debug)]
pub struct HistorySession {
    pub history: History,
    pub active_index: ActiveIndexState,
}

impl HistorySession {
    pub fn new(history: History, navigation_wrap: bool) -> Self {
        Self {
            history,
            active_index: ActiveIndexState::new(navigation_wrap),
        }
    }

    pub fn with_defaults(name: impl Into<String>) -> Self {
        Self::new(History::with_defaults(name), false)
    }

    pub fn ingest_text(
        &mut self,
        text: &str,
        rich: Option<RichPayload>,
        created_at: u64,
    ) -> IngestOutcome {
        let outcome = self.history.ingest_text(text, rich, created_at);
        match outcome {
            IngestOutcome::Added
            | IngestOutcome::MovedExisting { .. }
            | IngestOutcome::ReplacedGrowingLine { .. } => {
                self.active_index.on_external_ingest();
            }
            IngestOutcome::RejectedTextSize => {}
        }
        outcome
    }

    pub fn select(&mut self, uuid: uuid::Uuid) -> Result<usize> {
        let index = self.history.select(uuid)?;
        self.active_index.on_select();
        Ok(index)
    }

    pub fn select_at_offset(&mut self, offset: i32) -> Result<(usize, &HistoryItem)> {
        let index = self
            .active_index
            .select_at_offset(self.history.len(), offset)?;
        let item = self
            .history
            .get(index)
            .ok_or(Error::ActiveIndexOutOfRange {
                index,
                len: self.history.len(),
            })?;
        Ok((index, item))
    }

    /// True when clipboard text is already the active history entry (navigation echo).
    pub fn clipboard_echoes_active_item(&self, text: &str) -> bool {
        self.history()
            .get(self.active_index().active_index())
            .and_then(|item| item.plain_text())
            .is_some_and(|active| active == text)
    }

    pub fn pop(&mut self) -> Option<HistoryItem> {
        let item = self.history.pop();
        if item.is_some() {
            self.active_index.on_pop();
        }
        item
    }

    pub fn delete(&mut self, uuid: uuid::Uuid) -> Result<HistoryItem> {
        let (item, deleted_index) = self.history.delete(uuid)?;
        self.active_index
            .on_delete(deleted_index, self.history.len());
        Ok(item)
    }

    pub fn empty(&mut self) {
        self.history.empty();
        self.active_index.on_empty_history();
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn active_index(&self) -> &ActiveIndexState {
        &self.active_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_keeps_active_index_at_zero_after_ingest() {
        let mut session = HistorySession::with_defaults("history");
        session.active_index.set_active_index(2, 3).unwrap();
        session.ingest_text("fresh", None, 1);
        assert_eq!(session.active_index.active_index(), 0);
    }

    #[test]
    fn clipboard_echoes_active_item_matches_plain_text() {
        let mut session = HistorySession::with_defaults("history");
        session.ingest_text("alpha", None, 1);
        session.ingest_text("beta", None, 2);
        session.active_index.set_active_index(0, 2).unwrap();
        assert!(session.clipboard_echoes_active_item("beta"));
        assert!(!session.clipboard_echoes_active_item("alpha"));
    }

    #[test]
    fn session_prev_next_offsets_clipboard_target() {
        let mut session = HistorySession::with_defaults("history");
        session.ingest_text("one", None, 1);
        session.ingest_text("two", None, 2);
        session.ingest_text("three", None, 3);

        let (index, item) = session.select_at_offset(1).unwrap();
        assert_eq!(index, 1);
        assert_eq!(item.plain_text(), Some("two"));

        let (index, item) = session.select_at_offset(-1).unwrap();
        assert_eq!(index, 0);
        assert_eq!(item.plain_text(), Some("three"));
    }
}