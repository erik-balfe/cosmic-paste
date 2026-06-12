//! cosmic_config hot-reload for the daemon.

use std::sync::Arc;

use cosmic_paste_core::dbus::state::SharedDaemonState;
use cosmic_paste_core::Settings;
use notify::RecommendedWatcher;
use tokio::sync::mpsc;

use crate::monitor::MonitorConfig;

pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
}

pub fn spawn_config_watcher(
    state: SharedDaemonState,
    monitor_config: Arc<std::sync::Mutex<MonitorConfig>>,
) -> Option<ConfigWatcher> {
    let config = Settings::config().ok()?;
    let (reload_tx, mut reload_rx) = mpsc::unbounded_channel::<Vec<String>>();

    let watcher = config
        .watch(move |_cfg, keys| {
            let keys = keys.iter().map(ToString::to_string).collect();
            let _ = reload_tx.send(keys);
        })
        .inspect_err(|err| tracing::warn!("failed to watch cosmic-paste settings: {err}"))
        .ok()?;

    tokio::spawn(async move {
        while let Some(keys) = reload_rx.recv().await {
            let watch_primary = {
                let mut guard = state.lock().await;
                guard.apply_settings_keys(&keys);
                guard.settings.primary_to_history
            };

            if keys.iter().any(|key| key == "primary_to_history")
                && let Ok(mut cfg) = monitor_config.lock()
            {
                cfg.watch_primary = watch_primary;
                tracing::debug!(watch_primary, "updated monitor config");
            }
        }
    });

    Some(ConfigWatcher { _watcher: watcher })
}