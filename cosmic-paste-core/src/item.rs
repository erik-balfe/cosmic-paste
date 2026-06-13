use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryItem {
    pub uuid: Uuid,
    pub kind: ItemKind,
    pub display: String,
    pub created_at: u64,
    pub byte_size: u64,
    pub password_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ItemKind {
    Text {
        plain: String,
        rich: Option<RichPayload>,
    },
    UriList(Vec<String>),
    Image {
        checksum: [u8; 32],
        path: PathBuf,
    },
    Color {
        rgba: [f32; 4],
    },
    Password {
        name: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RichPayload {
    pub html: Option<String>,
    pub xml: Option<String>,
}

impl HistoryItem {
    pub fn new_text(plain: String, rich: Option<RichPayload>, created_at: u64) -> Self {
        let byte_size = plain.len() as u64
            + rich
                .as_ref()
                .map(|r| {
                    r.html.as_ref().map(|s| s.len()).unwrap_or(0)
                        + r.xml.as_ref().map(|s| s.len()).unwrap_or(0)
                })
                .unwrap_or(0) as u64;
        let display = format_display_line(&plain, 60);

        Self {
            uuid: Uuid::new_v4(),
            kind: ItemKind::Text {
                plain,
                rich,
            },
            display,
            created_at,
            byte_size,
            password_name: None,
        }
    }

    pub fn plain_text(&self) -> Option<&str> {
        match &self.kind {
            ItemKind::Text { plain, .. } => Some(plain.as_str()),
            _ => None,
        }
    }

    pub fn is_password(&self) -> bool {
        matches!(self.kind, ItemKind::Password { .. })
    }
}

/// Collapse whitespace and newlines for single-line UI labels.
pub fn collapse_display_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn truncate_display(text: &str, max_len: usize) -> String {
    truncate_chars_end(text, max_len)
}

pub fn truncate_display_middle(text: &str, max_len: usize) -> String {
    truncate_chars_middle(text, max_len)
}

/// Single-line preview for lists, tooltips, and panel popups.
pub fn format_display_line(text: &str, max_len: usize) -> String {
    truncate_chars_end(&collapse_display_text(text), max_len)
}

/// Single-line preview with start and end preserved when truncated.
pub fn format_display_line_middle(text: &str, max_len: usize) -> String {
    truncate_chars_middle(&collapse_display_text(text), max_len)
}

fn truncate_chars_end(text: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }

    let mut chars = text.chars();
    let preview: String = chars.by_ref().take(max_len).collect();
    if chars.next().is_some() {
        preview + "…"
    } else {
        preview
    }
}

fn truncate_chars_middle(text: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }

    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_len {
        return chars.into_iter().collect();
    }
    if max_len == 1 {
        return "…".to_string();
    }

    let keep = max_len - 1;
    let head_len = keep / 2;
    let tail_len = keep - head_len;
    let tail_start = chars.len() - tail_len;
    let mut out = String::with_capacity(max_len);
    out.extend(chars.iter().take(head_len));
    out.push('…');
    out.extend(chars.iter().skip(tail_start));
    out
}

pub fn text_checksum(plain: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(plain.as_bytes());
    digest.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_display_line_collapses_newlines() {
        let line = format_display_line("first line\nsecond line", 80);
        assert_eq!(line, "first line second line");
    }

    #[test]
    fn format_display_line_middle_keeps_ends() {
        let line = format_display_line_middle("abcdefghijklmnopqrstuvwxyz", 10);
        assert_eq!(line, "abcd…vwxyz");
    }
}