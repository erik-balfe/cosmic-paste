//! Embedded monochrome panel icon (tinted by libcosmic symbolic styling).

use cosmic::widget::icon;

pub const PASTE_SVG: &[u8] = include_bytes!("../icons/paste-symbolic.svg");

pub fn paste_handle() -> icon::Handle {
    icon::from_svg_bytes(PASTE_SVG).symbolic(true)
}