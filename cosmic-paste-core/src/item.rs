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
        let display = truncate_display(&plain, 60);

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

pub fn truncate_display(text: &str, max_len: usize) -> String {
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

pub fn text_checksum(plain: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(plain.as_bytes());
    digest.into()
}