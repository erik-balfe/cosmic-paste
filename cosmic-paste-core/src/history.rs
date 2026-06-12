use uuid::Uuid;

use crate::error::{Error, Result};
use crate::item::{text_checksum, HistoryItem, ItemKind, RichPayload};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HistoryPolicies {
    pub max_history_size: usize,
    pub max_memory_usage_bytes: u64,
    pub max_text_item_size: usize,
    pub min_text_item_size: usize,
    pub growing_lines: bool,
    pub trim_items: bool,
    pub element_display_size: usize,
}

impl Default for HistoryPolicies {
    fn default() -> Self {
        Self {
            max_history_size: 100,
            max_memory_usage_bytes: 30 * 1024 * 1024,
            max_text_item_size: 1_048_575,
            min_text_item_size: 1,
            growing_lines: false,
            trim_items: false,
            element_display_size: 60,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum IngestOutcome {
    Added,
    MovedExisting { uuid: Uuid },
    ReplacedGrowingLine { uuid: Uuid },
    RejectedTextSize,
}

#[derive(Clone, Debug)]
pub struct History {
    name: String,
    items: Vec<HistoryItem>,
    policies: HistoryPolicies,
    byte_total: u64,
}

impl History {
    pub fn new(name: impl Into<String>, policies: HistoryPolicies) -> Self {
        Self {
            name: name.into(),
            items: Vec::new(),
            policies,
            byte_total: 0,
        }
    }

    pub fn with_defaults(name: impl Into<String>) -> Self {
        Self::new(name, HistoryPolicies::default())
    }

    /// Rebuild history from persisted items (recalculates `byte_total`).
    pub fn from_persisted(
        name: impl Into<String>,
        policies: HistoryPolicies,
        items: Vec<HistoryItem>,
    ) -> Self {
        let byte_total = items.iter().map(|item| item.byte_size).sum();
        Self {
            name: name.into(),
            items,
            policies,
            byte_total,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[HistoryItem] {
        &self.items
    }

    pub fn get(&self, index: usize) -> Option<&HistoryItem> {
        self.items.get(index)
    }

    pub fn byte_total(&self) -> u64 {
        self.byte_total
    }

    pub fn policies(&self) -> &HistoryPolicies {
        &self.policies
    }

    pub fn set_policies(&mut self, policies: HistoryPolicies) {
        self.policies = policies;
        self.enforce_limits();
    }

    pub fn find_index(&self, uuid: Uuid) -> Option<usize> {
        self.items.iter().position(|item| item.uuid == uuid)
    }

    pub fn ingest_text(
        &mut self,
        text: &str,
        rich: Option<RichPayload>,
        created_at: u64,
    ) -> IngestOutcome {
        let prepared = self.prepare_text(text);
        if prepared.is_empty() {
            return IngestOutcome::RejectedTextSize;
        }

        if prepared.len() < self.policies.min_text_item_size
            || prepared.len() > self.policies.max_text_item_size
        {
            return IngestOutcome::RejectedTextSize;
        }

        if self.policies.growing_lines
            && let Some(uuid) = self.try_growing_lines_replace(&prepared, rich.clone(), created_at)
        {
            return IngestOutcome::ReplacedGrowingLine { uuid };
        }

        let checksum = text_checksum(&prepared);
        if let Some(index) = self.find_text_by_checksum(checksum) {
            let uuid = self.items[index].uuid;
            self.move_to_front(index);
            return IngestOutcome::MovedExisting { uuid };
        }

        let mut item = HistoryItem::new_text(prepared, rich, created_at);
        item.display = crate::item::truncate_display(
            item.plain_text().unwrap_or_default(),
            self.policies.element_display_size,
        );
        self.push_front(item);
        IngestOutcome::Added
    }

    pub fn select(&mut self, uuid: Uuid) -> Result<usize> {
        let index = self
            .find_index(uuid)
            .ok_or(Error::NotFound(uuid))?;
        self.move_to_front(index);
        Ok(0)
    }

    /// GPaste-compatible: always removes the newest entry (index 0).
    pub fn pop(&mut self) -> Option<HistoryItem> {
        if self.items.is_empty() {
            return None;
        }
        let item = self.items.remove(0);
        self.byte_total = self.byte_total.saturating_sub(item.byte_size);
        Some(item)
    }

    pub fn delete(&mut self, uuid: Uuid) -> Result<(HistoryItem, usize)> {
        let index = self
            .find_index(uuid)
            .ok_or(Error::NotFound(uuid))?;
        let item = self.items.remove(index);
        self.byte_total = self.byte_total.saturating_sub(item.byte_size);
        Ok((item, index))
    }

    pub fn empty(&mut self) {
        self.items.clear();
        self.byte_total = 0;
    }

    fn prepare_text(&self, text: &str) -> String {
        if self.policies.trim_items {
            text.trim().to_owned()
        } else {
            text.to_owned()
        }
    }

    fn find_text_by_checksum(&self, checksum: [u8; 32]) -> Option<usize> {
        self.items.iter().position(|item| {
            item.plain_text()
                .is_some_and(|plain| text_checksum(plain) == checksum)
        })
    }

    fn try_growing_lines_replace(
        &mut self,
        new_text: &str,
        rich: Option<RichPayload>,
        created_at: u64,
    ) -> Option<Uuid> {
        let first = self.items.first()?;
        let previous = first.plain_text()?;
        if !new_text.starts_with(previous) || new_text.len() <= previous.len() {
            return None;
        }

        let uuid = first.uuid;
        let byte_size = new_text.len() as u64
            + rich
                .as_ref()
                .map(|r| {
                    r.html.as_ref().map(|s| s.len()).unwrap_or(0)
                        + r.xml.as_ref().map(|s| s.len()).unwrap_or(0)
                })
                .unwrap_or(0) as u64;
        let display = crate::item::truncate_display(new_text, self.policies.element_display_size);

        self.byte_total = self
            .byte_total
            .saturating_sub(self.items[0].byte_size)
            .saturating_add(byte_size);
        self.items[0] = HistoryItem {
            uuid,
            kind: ItemKind::Text {
                plain: new_text.to_owned(),
                rich,
            },
            display,
            created_at,
            byte_size,
            password_name: None,
        };
        Some(uuid)
    }

    fn push_front(&mut self, item: HistoryItem) {
        self.byte_total = self.byte_total.saturating_add(item.byte_size);
        self.items.insert(0, item);
        self.enforce_limits();
    }

    fn move_to_front(&mut self, index: usize) {
        if index == 0 {
            return;
        }
        let item = self.items.remove(index);
        self.items.insert(0, item);
    }

    fn enforce_limits(&mut self) {
        while self.items.len() > self.policies.max_history_size {
            if let Some(item) = self.items.pop() {
                self.byte_total = self.byte_total.saturating_sub(item.byte_size);
            }
        }

        while self.byte_total > self.policies.max_memory_usage_bytes {
            if let Some(item) = self.items.pop() {
                self.byte_total = self.byte_total.saturating_sub(item.byte_size);
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_policies() -> HistoryPolicies {
        HistoryPolicies {
            max_history_size: 5,
            max_memory_usage_bytes: 1024,
            max_text_item_size: 256,
            min_text_item_size: 1,
            growing_lines: false,
            trim_items: false,
            element_display_size: 20,
        }
    }

    #[test]
    fn ingest_adds_newest_at_front() {
        let mut history = History::new("history", sample_policies());
        assert!(matches!(
            history.ingest_text("alpha", None, 1),
            IngestOutcome::Added
        ));
        assert!(matches!(
            history.ingest_text("beta", None, 2),
            IngestOutcome::Added
        ));
        assert_eq!(history.get(0).unwrap().plain_text(), Some("beta"));
        assert_eq!(history.get(1).unwrap().plain_text(), Some("alpha"));
    }

    #[test]
    fn duplicate_moves_existing_to_front() {
        let mut history = History::new("history", sample_policies());
        history.ingest_text("alpha", None, 1);
        history.ingest_text("beta", None, 2);
        let outcome = history.ingest_text("alpha", None, 3);
        assert!(matches!(outcome, IngestOutcome::MovedExisting { .. }));
        assert_eq!(history.len(), 2);
        assert_eq!(history.get(0).unwrap().plain_text(), Some("alpha"));
    }

    #[test]
    fn growing_lines_replaces_prefix_entry() {
        let mut policies = sample_policies();
        policies.growing_lines = true;
        let mut history = History::new("history", policies);
        history.ingest_text("foo", None, 1);
        let outcome = history.ingest_text("foo bar", None, 2);
        assert!(matches!(outcome, IngestOutcome::ReplacedGrowingLine { .. }));
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap().plain_text(), Some("foo bar"));
    }

    #[test]
    fn rejects_text_outside_bounds() {
        let mut policies = sample_policies();
        policies.min_text_item_size = 3;
        let mut history = History::new("history", policies);
        assert!(matches!(
            history.ingest_text("ab", None, 1),
            IngestOutcome::RejectedTextSize
        ));
    }

    #[test]
    fn trim_items_before_ingest() {
        let mut policies = sample_policies();
        policies.trim_items = true;
        let mut history = History::new("history", policies);
        history.ingest_text("  spaced  ", None, 1);
        assert_eq!(history.get(0).unwrap().plain_text(), Some("spaced"));
    }

    #[test]
    fn pop_removes_newest_entry() {
        let mut history = History::new("history", sample_policies());
        history.ingest_text("alpha", None, 1);
        history.ingest_text("beta", None, 2);
        let popped = history.pop().unwrap();
        assert_eq!(popped.plain_text(), Some("beta"));
        assert_eq!(history.get(0).unwrap().plain_text(), Some("alpha"));
    }

    #[test]
    fn select_moves_item_to_front() {
        let mut history = History::new("history", sample_policies());
        history.ingest_text("alpha", None, 1);
        history.ingest_text("beta", None, 2);
        let uuid = history.get(1).unwrap().uuid;
        assert_eq!(history.select(uuid).unwrap(), 0);
        assert_eq!(history.get(0).unwrap().plain_text(), Some("alpha"));
    }

    #[test]
    fn enforces_max_history_size() {
        let mut policies = sample_policies();
        policies.max_history_size = 2;
        let mut history = History::new("history", policies);
        history.ingest_text("one", None, 1);
        history.ingest_text("two", None, 2);
        history.ingest_text("three", None, 3);
        assert_eq!(history.len(), 2);
        assert_eq!(history.get(0).unwrap().plain_text(), Some("three"));
        assert_eq!(history.get(1).unwrap().plain_text(), Some("two"));
    }

    #[test]
    fn enforces_memory_budget() {
        let mut policies = sample_policies();
        policies.max_history_size = 100;
        policies.max_memory_usage_bytes = 5;
        let mut history = History::new("history", policies);
        history.ingest_text("12345", None, 1);
        history.ingest_text("67890", None, 2);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap().plain_text(), Some("67890"));
    }
}