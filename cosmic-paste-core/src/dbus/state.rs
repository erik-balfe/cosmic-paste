use std::sync::mpsc::SyncSender;
use std::sync::Arc;

use cosmic_config::CosmicConfigEntry;
use tokio::sync::Mutex;

use crate::dbus::clipboard::ClipboardWriteSender;
use crate::dbus::guard::SharedSelfCopyGuard;
use crate::dbus::lifecycle::LifecycleHandle;
use crate::dbus::service::CosmicPasteService;
use crate::persistence::{DataPaths, HistoryStore, LoadHistoryOutcome, SessionState};
use crate::settings::Settings;
use crate::{History, HistorySession};

pub type SharedDaemonState = Arc<Mutex<DaemonState>>;

pub struct DaemonState {
    pub session: HistorySession,
    pub settings: Settings,
    pub tracking: bool,
    pub applet_present: bool,
    pub portal_shortcuts_available: bool,
    clipboard_write: Option<ClipboardWriteSender>,
    pub self_copy_guard: SharedSelfCopyGuard,
    /// Test harness: skip wl-copy, acknowledge writes in-process.
    #[cfg(test)]
    pub ack_clipboard_writes: bool,
    store: HistoryStore,
}

impl DaemonState {
    pub fn new_in_memory() -> Self {
        // In-memory harnesses run tests in parallel in one process; skip disk I/O.
        let settings = Settings {
            save_history: false,
            ..Settings::default()
        };
        let mut state = Self {
            session: HistorySession::with_defaults(settings.history_name.clone()),
            settings,
            tracking: true,
            applet_present: false,
            portal_shortcuts_available: false,
            clipboard_write: None,
            self_copy_guard: Arc::new(std::sync::Mutex::new(crate::dbus::SelfCopyGuard::new())),
            #[cfg(test)]
            ack_clipboard_writes: false,
            store: HistoryStore::new(DataPaths::new(
                std::env::temp_dir().join(format!("cosmic-paste-test-{}", std::process::id())),
            )),
        };
        let _ = state.store.ensure_dirs();
        state.apply_settings();
        state
    }

    pub fn save_history(&self) -> bool {
        self.settings.save_history
    }

    pub fn apply_settings(&mut self) {
        self.tracking = self.settings.track_changes;
        self.session
            .history
            .set_policies(self.settings.history_policies());
        self.session
            .active_index
            .set_navigation_wrap(self.settings.navigation_wrap);
    }

    pub fn apply_settings_keys(&mut self, keys: &[String]) {
        if let Ok(config) = Settings::config() {
            let (errors, changed) = self.settings.update_keys(&config, keys);
            for err in errors {
                tracing::warn!("settings reload error: {err}");
            }
            if !changed.is_empty() {
                tracing::info!(?changed, "reloaded cosmic-paste settings");
                self.apply_settings();
            }
        }
    }

    pub fn set_clipboard_writer(&mut self, tx: SyncSender<crate::dbus::ClipboardWriteRequest>) {
        self.clipboard_write = Some(Arc::new(tx));
    }

    pub fn clipboard_writer(&self) -> Option<&ClipboardWriteSender> {
        self.clipboard_write.as_ref()
    }

    pub fn load_default() -> crate::persistence::PersistenceResult<Self> {
        let paths = DataPaths::default_xdg()?;
        let store = HistoryStore::new(paths);
        store.ensure_dirs()?;

        let settings = Settings::load();
        let session_state = store.load_session_state()?;
        let policies = settings.history_policies();
        let history = match store.load_history(&session_state.current_history, policies)? {
            LoadHistoryOutcome::Loaded(history)
            | LoadHistoryOutcome::RecoveredFromBackup(history) => history,
            LoadHistoryOutcome::EmptyAfterCorruption { name } => History::with_defaults(name),
        };

        let mut session = HistorySession::new(history, settings.navigation_wrap);
        let _ = session
            .active_index
            .set_active_index(session_state.active_index, session.history.len());

        let mut state = Self {
            session,
            settings,
            tracking: true,
            applet_present: false,
            portal_shortcuts_available: false,
            clipboard_write: None,
            self_copy_guard: Arc::new(std::sync::Mutex::new(crate::dbus::SelfCopyGuard::new())),
            #[cfg(test)]
            ack_clipboard_writes: false,
            store,
        };
        state.apply_settings();
        Ok(state)
    }

    pub fn set_self_copy_guard(&mut self, guard: SharedSelfCopyGuard) {
        self.self_copy_guard = guard;
    }

    pub fn on_applet_present_changed(&mut self, present: bool) {
        self.applet_present = present;
        if self.settings.track_applet_state {
            self.tracking = present && self.settings.track_changes;
            tracing::debug!(
                present,
                tracking = self.tracking,
                "applied track_applet_state policy"
            );
        }
    }

    pub fn service(self, lifecycle: LifecycleHandle) -> CosmicPasteService {
        CosmicPasteService::new(Arc::new(Mutex::new(self)), lifecycle)
    }

    pub fn persist(&self) -> crate::persistence::PersistenceResult<()> {
        if !self.save_history() {
            return Ok(());
        }
        self.store.save_history(self.session.history())?;
        self.store.save_session_state(&SessionState {
            active_index: self.session.active_index().active_index(),
            current_history: self.session.history().name().to_owned(),
        })
    }

    pub fn session_mut(&mut self) -> &mut HistorySession {
        &mut self.session
    }

    pub fn history(&self) -> &History {
        self.session.history()
    }
}