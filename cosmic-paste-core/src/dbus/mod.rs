//! DBus API constants, daemon state, service implementation, and client proxy.

pub mod client;
pub mod service;
pub mod state;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BUS_NAME: &str = "org.system76.CosmicPaste";
pub const OBJECT_PATH: &str = "/org/system76/CosmicPaste";
pub const INTERFACE_NAME: &str = "org.system76.CosmicPaste2";

pub fn item_kind_name(kind: &crate::ItemKind) -> &'static str {
    match kind {
        crate::ItemKind::Text { .. } => "text",
        crate::ItemKind::UriList(_) => "uri",
        crate::ItemKind::Image { .. } => "image",
        crate::ItemKind::Color { .. } => "color",
        crate::ItemKind::Password { .. } => "password",
    }
}

pub fn element_value(item: &crate::HistoryItem) -> String {
    match &item.kind {
        crate::ItemKind::Text { plain, .. } => plain.clone(),
        crate::ItemKind::UriList(uris) => uris.join("\n"),
        crate::ItemKind::Image { checksum, .. } => crate::checksum_hex(checksum),
        crate::ItemKind::Color { rgba } => format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            (rgba[0] * 255.0) as u8,
            (rgba[1] * 255.0) as u8,
            (rgba[2] * 255.0) as u8,
            (rgba[3] * 255.0) as u8
        ),
        crate::ItemKind::Password { name } => name.clone(),
    }
}

pub fn parse_uuid(uuid: &str) -> zbus::fdo::Result<uuid::Uuid> {
    uuid.parse()
        .map_err(|_| zbus::fdo::Error::InvalidArgs(format!("invalid uuid: {uuid}")))
}