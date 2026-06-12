//! cosmic_config settings for cosmic-paste (`docs/DESIGN.md` §7).

use cosmic_config::{CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

use crate::HistoryPolicies;

pub const APP_ID: &str = "com.system76.CosmicPaste";

/// Global shortcut accelerators (empty string disables a binding).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ShortcutSettings {
    pub show_history: String,
    pub launch_ui: String,
    pub pop: String,
    pub sync_clipboard_to_primary: String,
    pub sync_primary_to_clipboard: String,
    pub mark_password: String,
    pub select_previous: String,
    pub select_next: String,
    pub quick_select_0: String,
    pub quick_select_1: String,
    pub quick_select_2: String,
    pub quick_select_3: String,
    pub quick_select_4: String,
    pub quick_select_5: String,
    pub quick_select_6: String,
    pub quick_select_7: String,
    pub quick_select_8: String,
    pub quick_select_9: String,
}

impl ShortcutSettings {
    pub fn default_bindings() -> Self {
        Self {
            show_history: "<Ctrl><Alt>H".into(),
            launch_ui: "<Ctrl><Alt>G".into(),
            pop: "<Ctrl><Alt>V".into(),
            sync_clipboard_to_primary: "<Ctrl><Alt>O".into(),
            sync_primary_to_clipboard: "<Ctrl><Alt>P".into(),
            mark_password: "<Ctrl><Alt>S".into(),
            select_previous: "<Ctrl><Alt>Up".into(),
            select_next: "<Ctrl><Alt>Down".into(),
            ..Self::default()
        }
    }
}

/// Persistent cosmic-paste preferences (cosmic_config v1).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, CosmicConfigEntry)]
#[version = 1]
pub struct Settings {
    pub element_size: u16,
    pub growing_lines: bool,
    pub history_name: String,
    pub images_support: bool,
    pub images_preview: bool,
    pub images_preview_size: u16,
    pub close_on_select: bool,
    pub open_centered: bool,
    pub max_displayed_history_size: u8,
    pub max_history_size: u16,
    pub max_memory_usage_mb: u16,
    pub max_text_item_size: u32,
    pub min_text_item_size: u16,
    pub primary_to_history: bool,
    pub rich_text_support: bool,
    pub save_history: bool,
    pub track_changes: bool,
    pub trim_items: bool,
    pub synchronize_clipboards: bool,
    pub empty_history_confirmation: bool,
    pub navigation_wrap: bool,
    pub track_applet_state: bool,
    pub screensaver_restore_clipboard: bool,
    #[serde(default)]
    pub excluded_targets: Vec<String>,
    #[serde(default)]
    pub shortcuts: ShortcutSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            element_size: 60,
            growing_lines: false,
            history_name: "history".into(),
            images_support: false,
            images_preview: true,
            images_preview_size: 50,
            close_on_select: true,
            open_centered: false,
            max_displayed_history_size: 20,
            max_history_size: 100,
            max_memory_usage_mb: 30,
            max_text_item_size: 1_048_575,
            min_text_item_size: 1,
            primary_to_history: false,
            rich_text_support: true,
            save_history: true,
            track_changes: true,
            trim_items: false,
            synchronize_clipboards: false,
            empty_history_confirmation: true,
            navigation_wrap: false,
            track_applet_state: false,
            screensaver_restore_clipboard: false,
            excluded_targets: Vec::new(),
            shortcuts: ShortcutSettings::default_bindings(),
        }
    }
}

impl Settings {
    pub fn config() -> Result<cosmic_config::Config, cosmic_config::Error> {
        cosmic_config::Config::new(APP_ID, Self::VERSION)
    }

    pub fn load() -> Self {
        let Ok(config) = Self::config() else {
            tracing::warn!("cosmic config directory unavailable; using defaults");
            return Self::default();
        };

        match Self::get_entry(&config) {
            Ok(settings) => settings,
            Err((errors, fallback)) => {
                if !errors.is_empty() {
                    tracing::warn!("settings parse errors: {errors:?}");
                }
                fallback
            }
        }
    }

    pub fn history_policies(&self) -> HistoryPolicies {
        HistoryPolicies {
            max_history_size: self.max_history_size as usize,
            max_memory_usage_bytes: u64::from(self.max_memory_usage_mb) * 1024 * 1024,
            max_text_item_size: self.max_text_item_size as usize,
            min_text_item_size: self.min_text_item_size as usize,
            growing_lines: self.growing_lines,
            trim_items: self.trim_items,
            element_display_size: self.element_size as usize,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_navigation_wrap_is_clamp() {
        assert!(!Settings::default().navigation_wrap);
    }

    #[test]
    fn history_policies_map_from_settings() {
        let settings = Settings {
            max_history_size: 42,
            max_memory_usage_mb: 7,
            ..Settings::default()
        };
        let policies = settings.history_policies();
        assert_eq!(policies.max_history_size, 42);
        assert_eq!(policies.max_memory_usage_bytes, 7 * 1024 * 1024);
    }

    #[test]
    fn quick_select_shortcuts_disabled_by_default() {
        let shortcuts = Settings::default().shortcuts;
        assert!(shortcuts.quick_select_0.is_empty());
        assert!(shortcuts.quick_select_9.is_empty());
    }
}