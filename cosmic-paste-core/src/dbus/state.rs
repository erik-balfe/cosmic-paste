use std::sync::{Arc, mpsc};

use tokio::sync::Mutex;

use crate::dbus::clipboard::ClipboardWriteSender;
use crate::dbus::lifecycle::LifecycleHandle;
use crate::dbus::service::CosmicPasteService;
use crate::persistence::{DataPaths, HistoryStore, LoadHistoryOutcome, SessionState};
use crate::{History, HistoryPolicies, HistorySession};

pub type SharedDaemonState = Arc<Mutex<DaemonState>>;

pub struct DaemonState {
    pub session: HistorySession,
    pub tracking: bool,
    pub applet_present: bool,
    pub portal_shortcuts_available: bool,
    clipboard_write: Option<ClipboardWriteSender>,
    store: HistoryStore,
}

impl DaemonState {
    pub fn new_in_memory() -> Self {
        Self {
            session: HistorySession::with_defaults("history"),
            tracking: true,
            applet_present: false,
            portal_shortcuts_available: false,
            clipboard_write: None,
            store: HistoryStore::new(DataPaths::new(std::env::temp_dir().join("cosmic-paste-test"))),
        }
    }

    pub fn set_clipboard_writer(&mut self, tx: mpsc::Sender<crate::dbus::ClipboardWriteRequest>) {
        self.clipboard_write = Some(Arc::new(tx));
    }

    pub fn clipboard_writer(&self) -> Option<&ClipboardWriteSender> {
        self.clipboard_write.as_ref()
    }

    pub fn load_default() -> crate::persistence::PersistenceResult<Self> {
        let paths = DataPaths::default_xdg()?;
        let store = HistoryStore::new(paths);
        store.ensure_dirs()?;

        let session_state = store.load_session_state()?;
        let policies = HistoryPolicies::default();
        let history = match store.load_history(&session_state.current_history, policies)? {
            LoadHistoryOutcome::Loaded(history)
            | LoadHistoryOutcome::RecoveredFromBackup(history) => history,
            LoadHistoryOutcome::EmptyAfterCorruption { name } => History::with_defaults(name),
        };

        let mut session = HistorySession::new(history, false);
        let _ = session
            .active_index
            .set_active_index(session_state.active_index, session.history.len());

        Ok(Self {
            session,
            tracking: true,
            applet_present: false,
            portal_shortcuts_available: false,
            clipboard_write: None,
            store,
        })
    }

    pub fn service(self, lifecycle: LifecycleHandle) -> CosmicPasteService {
        CosmicPasteService::new(Arc::new(Mutex::new(self)), lifecycle)
    }

    pub fn persist(&self) -> crate::persistence::PersistenceResult<()> {
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