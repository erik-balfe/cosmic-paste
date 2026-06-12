use std::sync::Arc;

use zbus::interface;
use zbus::object_server::SignalEmitter;

use super::clipboard::write_clipboard;
use super::lifecycle::{LifecycleHandle, ShutdownReason};
use super::state::SharedDaemonState;
use super::{element_value, item_kind_name, parse_uuid, VERSION};
use crate::error::Error;
use crate::History;

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn map_error(err: Error) -> zbus::fdo::Error {
    match err {
        Error::NotFound(uuid) => zbus::fdo::Error::InvalidArgs(format!("not found: {uuid}")),
        Error::EmptyHistory => zbus::fdo::Error::Failed("history is empty".into()),
        Error::NavigationBoundary { index, len } => {
            zbus::fdo::Error::Failed(format!("navigation boundary (index {index}, len {len})"))
        }
        Error::ActiveIndexOutOfRange { index, len } => {
            zbus::fdo::Error::InvalidArgs(format!("index {index} out of range for len {len}"))
        }
        Error::TextSizeOutOfBounds { len, min, max } => zbus::fdo::Error::InvalidArgs(format!(
            "text length {len} is outside allowed bounds ({min}..={max})"
        )),
    }
}

fn history_entries(history: &History) -> Vec<(String, String)> {
    history
        .items()
        .iter()
        .map(|item| (item.uuid.to_string(), item.display.clone()))
        .collect()
}

fn not_supported(method: &str) -> zbus::fdo::Error {
    zbus::fdo::Error::NotSupported(format!("{method} is not implemented yet"))
}

/// GPaste2-compatible DBus service for cosmic-paste.
pub struct CosmicPasteService {
    state: SharedDaemonState,
    lifecycle: LifecycleHandle,
}

impl CosmicPasteService {
    pub fn new(state: SharedDaemonState, lifecycle: LifecycleHandle) -> Self {
        Self { state, lifecycle }
    }

    pub fn shared_state(&self) -> SharedDaemonState {
        self.state.clone()
    }

    async fn with_state<F, T>(&self, f: F) -> zbus::fdo::Result<T>
    where
        F: FnOnce(&mut super::state::DaemonState) -> zbus::fdo::Result<T>,
    {
        let mut guard = self.state.lock().await;
        f(&mut guard)
    }
}

#[interface(interface = "org.system76.CosmicPaste2")]
impl CosmicPasteService {
    async fn add(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        text: &str,
    ) -> zbus::fdo::Result<()> {
        let (uuid, index) = self
            .with_state(|state| {
                if !state.tracking {
                    return Err(zbus::fdo::Error::Failed(
                        "clipboard tracking is disabled".into(),
                    ));
                }

                let outcome = state
                    .session_mut()
                    .ingest_text(text, None, unix_now());
                if matches!(outcome, crate::IngestOutcome::RejectedTextSize) {
                    return Err(zbus::fdo::Error::InvalidArgs(
                        "text size out of bounds".into(),
                    ));
                }

                let item = state
                    .history()
                    .get(0)
                    .ok_or_else(|| zbus::fdo::Error::Failed("failed to read added item".into()))?;
                let uuid = item.uuid.to_string();
                state.persist().map_err(|e| {
                    zbus::fdo::Error::Failed(format!("failed to persist history: {e}"))
                })?;
                Ok((uuid, 0u32))
            })
            .await?;

        let count = self.history_count().await?;
        Self::update(emitter.clone(), "add", &uuid, index).await?;
        Self::emit_active_index_changed(emitter, index, count).await?;
        Ok(())
    }

    async fn get_history(&self) -> zbus::fdo::Result<Vec<(String, String)>> {
        self.with_state(|state| Ok(history_entries(state.history()))).await
    }

    async fn get_active_index(&self) -> zbus::fdo::Result<u32> {
        self.read_active_index().await
    }

    async fn set_active_index(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        index: u32,
    ) -> zbus::fdo::Result<()> {
        let count = self
            .with_state(|state| {
                let history_len = state.history().len();
                state
                    .session_mut()
                    .active_index
                    .set_active_index(index as usize, history_len)
                    .map_err(map_error)?;
                state.persist().map_err(|e| {
                    zbus::fdo::Error::Failed(format!("failed to persist session state: {e}"))
                })?;
                Ok(history_len as u32)
            })
            .await?;

        Self::emit_active_index_changed(emitter, index, count).await?;
        Ok(())
    }

    async fn select_at_offset(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        offset: i32,
    ) -> zbus::fdo::Result<String> {
        let (uuid, text, index, count, rollback) = self
            .with_state(|state| {
                let rollback = state.session.clone();
                let (index, item) = state
                    .session_mut()
                    .select_at_offset(offset)
                    .map_err(map_error)?;
                let uuid = item.uuid.to_string();
                let text = element_value(item);
                let history_len = state.history().len();
                Ok((uuid, text, index as u32, history_len as u32, rollback))
            })
            .await?;

        if let Err(err) = self.write_clipboard_text(&text).await {
            let _ = self
                .with_state(|state| {
                    state.session = rollback;
                    Ok(())
                })
                .await;
            return Err(err);
        }

        self.with_state(|state| {
            state.persist().map_err(|e| {
                zbus::fdo::Error::Failed(format!("failed to persist session state: {e}"))
            })
        })
        .await?;

        Self::update(emitter.clone(), "select", &uuid, index).await?;
        Self::emit_active_index_changed(emitter, index, count).await?;
        Ok(uuid)
    }

    async fn track(&self, tracking_state: bool) -> zbus::fdo::Result<()> {
        self.with_state(|state| {
            state.tracking = tracking_state;
            Ok(())
        })
        .await
    }

    async fn on_applet_state_changed(&self, present: bool) -> zbus::fdo::Result<()> {
        self.with_state(|state| {
            state.on_applet_present_changed(present);
            Ok(())
        })
        .await
    }

    async fn get_history_name(&self) -> zbus::fdo::Result<String> {
        self.with_state(|state| Ok(state.history().name().to_owned()))
            .await
    }

    async fn get_history_size(&self, name: &str) -> zbus::fdo::Result<u32> {
        self.with_state(|state| {
            if state.history().name() != name {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "unknown history: {name}"
                )));
            }
            Ok(state.history().len() as u32)
        })
        .await
    }

    async fn list_histories(&self) -> zbus::fdo::Result<Vec<String>> {
        self.with_state(|state| Ok(vec![state.history().name().to_owned()]))
            .await
    }

    async fn empty_history(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
    ) -> zbus::fdo::Result<()> {
        let history_name = self
            .with_state(|state| {
                if state.history().name() != name {
                    return Err(zbus::fdo::Error::InvalidArgs(format!(
                        "unknown history: {name}"
                    )));
                }
                state.session_mut().empty();
                state.persist().map_err(|e| {
                    zbus::fdo::Error::Failed(format!("failed to persist empty history: {e}"))
                })?;
                Ok(state.history().name().to_owned())
            })
            .await?;

        Self::notify_empty_history(emitter.clone(), &history_name).await?;
        Self::emit_active_index_changed(emitter, 0, 0).await?;
        Ok(())
    }

    async fn delete(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        uuid: &str,
    ) -> zbus::fdo::Result<()> {
        let parsed = parse_uuid(uuid)?;
        let (target, index, count) = self
            .with_state(|state| {
                let deleted_index = state
                    .history()
                    .find_index(parsed)
                    .ok_or_else(|| map_error(Error::NotFound(parsed)))?;
                let item = state.session_mut().delete(parsed).map_err(map_error)?;
                state.persist().map_err(|e| {
                    zbus::fdo::Error::Failed(format!("failed to persist delete: {e}"))
                })?;
                Ok((
                    item.uuid.to_string(),
                    deleted_index as u32,
                    state.history().len() as u32,
                ))
            })
            .await?;

        Self::update(emitter.clone(), "remove", &target, index).await?;
        let active = self.read_active_index().await?;
        Self::emit_active_index_changed(emitter, active, count).await?;
        Ok(())
    }

    async fn add_file(&self, _file: &str) -> zbus::fdo::Result<()> {
        Err(not_supported("AddFile"))
    }

    async fn add_password(&self, _name: &str, _password: &str) -> zbus::fdo::Result<()> {
        Err(not_supported("AddPassword"))
    }

    async fn backup_history(&self, _history: &str, _backup: &str) -> zbus::fdo::Result<()> {
        Err(not_supported("BackupHistory"))
    }

    async fn delete_history(&self, _name: &str) -> zbus::fdo::Result<()> {
        Err(not_supported("DeleteHistory"))
    }

    async fn delete_password(&self, _name: &str) -> zbus::fdo::Result<()> {
        Err(not_supported("DeletePassword"))
    }

    async fn get_element(&self, uuid: &str) -> zbus::fdo::Result<String> {
        let parsed = parse_uuid(uuid)?;
        self.with_state(|state| {
            let item = state
                .history()
                .items()
                .iter()
                .find(|item| item.uuid == parsed)
                .ok_or_else(|| map_error(Error::NotFound(parsed)))?;
            Ok(element_value(item))
        })
        .await
    }

    async fn get_element_at_index(&self, index: u32) -> zbus::fdo::Result<(String, String)> {
        self.with_state(|state| {
            let item = state
                .history()
                .get(index as usize)
                .ok_or_else(|| {
                    zbus::fdo::Error::InvalidArgs(format!("index {index} out of range"))
                })?;
            Ok((item.uuid.to_string(), element_value(item)))
        })
        .await
    }

    async fn get_element_kind(&self, uuid: &str) -> zbus::fdo::Result<String> {
        let parsed = parse_uuid(uuid)?;
        self.with_state(|state| {
            let item = state
                .history()
                .items()
                .iter()
                .find(|item| item.uuid == parsed)
                .ok_or_else(|| map_error(Error::NotFound(parsed)))?;
            Ok(item_kind_name(&item.kind).to_owned())
        })
        .await
    }

    async fn get_elements(&self, uuids: Vec<String>) -> zbus::fdo::Result<Vec<(String, String)>> {
        let mut out = Vec::with_capacity(uuids.len());
        for uuid in uuids {
            let value = self.get_element(&uuid).await?;
            out.push((uuid, value));
        }
        Ok(out)
    }

    async fn get_raw_element(&self, uuid: &str) -> zbus::fdo::Result<String> {
        self.get_element(uuid).await
    }

    async fn get_raw_history(&self) -> zbus::fdo::Result<Vec<(String, String)>> {
        self.get_history().await
    }

    async fn merge(
        &self,
        _decoration: &str,
        _separator: &str,
        _uuids: Vec<String>,
    ) -> zbus::fdo::Result<()> {
        Err(not_supported("Merge"))
    }

    async fn rename_password(&self, _old_name: &str, _new_name: &str) -> zbus::fdo::Result<()> {
        Err(not_supported("RenamePassword"))
    }

    async fn replace(&self, _uuid: &str, _contents: &str) -> zbus::fdo::Result<()> {
        Err(not_supported("Replace"))
    }

    async fn search(&self, _query: &str) -> zbus::fdo::Result<Vec<String>> {
        Err(not_supported("Search"))
    }

    async fn select(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        uuid: &str,
    ) -> zbus::fdo::Result<()> {
        let parsed = parse_uuid(uuid)?;
        let (target, text, index, count, rollback) = self
            .with_state(|state| {
                let rollback = state.session.clone();
                let index = state.session_mut().select(parsed).map_err(map_error)?;
                let item = state
                    .history()
                    .get(index)
                    .ok_or_else(|| zbus::fdo::Error::Failed("failed to read selected item".into()))?;
                let target = item.uuid.to_string();
                let text = element_value(item);
                let count = state.history().len() as u32;
                Ok((target, text, index as u32, count, rollback))
            })
            .await?;

        if let Err(err) = self.write_clipboard_text(&text).await {
            let _ = self
                .with_state(|state| {
                    state.session = rollback;
                    Ok(())
                })
                .await;
            return Err(err);
        }

        self.with_state(|state| {
            state.persist().map_err(|e| {
                zbus::fdo::Error::Failed(format!("failed to persist select: {e}"))
            })
        })
        .await?;

        Self::update(emitter.clone(), "select", &target, index).await?;
        Self::emit_active_index_changed(emitter, index, count).await?;
        Ok(())
    }

    async fn set_password(&self, _uuid: &str, _name: &str) -> zbus::fdo::Result<()> {
        Err(not_supported("SetPassword"))
    }

    async fn switch_history(&self, _name: &str) -> zbus::fdo::Result<()> {
        Err(not_supported("SwitchHistory"))
    }

    async fn show_history(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let present = self.state.lock().await.applet_present;
        if !present {
            return Err(zbus::fdo::Error::Failed(
                "applet not in panel — add COSMIC Paste to the panel, or use `cosmic-paste history`"
                    .into(),
            ));
        }
        Self::emit_show_history(emitter).await?;
        Ok(())
    }

    async fn reexecute(&self) -> zbus::fdo::Result<()> {
        self.with_state(|state| {
            state.persist().map_err(|err| {
                zbus::fdo::Error::Failed(format!("failed to flush state before reexecute: {err}"))
            })?;
            Ok(())
        })
        .await?;
        self.lifecycle.request(ShutdownReason::Reexecute);
        Ok(())
    }

    async fn about(&self) -> zbus::fdo::Result<()> {
        Err(not_supported("About"))
    }

    #[zbus(property)]
    async fn active(&self) -> bool {
        self.state.lock().await.tracking
    }

    #[zbus(property)]
    async fn version(&self) -> String {
        VERSION.to_owned()
    }

    #[zbus(property)]
    async fn active_index(&self) -> u32 {
        self.read_active_index().await.unwrap_or(0)
    }

    #[zbus(property)]
    async fn applet_present(&self) -> bool {
        self.state.lock().await.applet_present
    }

    #[zbus(property)]
    async fn portal_shortcuts_available(&self) -> bool {
        self.state.lock().await.portal_shortcuts_available
    }

    #[zbus(signal)]
    async fn update(
        emitter: SignalEmitter<'_>,
        action: &str,
        target: &str,
        index: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "ShowHistory")]
    async fn notify_show_history(emitter: SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal, name = "SwitchHistory")]
    async fn notify_switch_history(emitter: SignalEmitter<'_>, history: &str) -> zbus::Result<()>;

    #[zbus(signal, name = "EmptyHistory")]
    async fn notify_empty_history(emitter: SignalEmitter<'_>, history: &str) -> zbus::Result<()>;

    #[zbus(signal, name = "DeleteHistory")]
    async fn notify_delete_history(emitter: SignalEmitter<'_>, history: &str) -> zbus::Result<()>;

    #[zbus(signal, name = "ActiveIndexChanged")]
    async fn emit_active_index_changed(
        emitter: SignalEmitter<'_>,
        index: u32,
        count: u32,
    ) -> zbus::Result<()>;
}

impl CosmicPasteService {
    pub async fn emit_history_update(
        emitter: SignalEmitter<'_>,
        action: &str,
        target: &str,
        index: u32,
    ) -> zbus::Result<()> {
        Self::update(emitter, action, target, index).await
    }

    pub async fn emit_active_index(
        emitter: SignalEmitter<'_>,
        index: u32,
        count: u32,
    ) -> zbus::Result<()> {
        Self::emit_active_index_changed(emitter, index, count).await
    }

    pub async fn emit_show_history(emitter: SignalEmitter<'_>) -> zbus::Result<()> {
        Self::notify_show_history(emitter).await
    }

    async fn write_clipboard_text(&self, text: &str) -> zbus::fdo::Result<()> {
        let tx = self.state.lock().await.clipboard_writer().map(Arc::clone);
        let Some(tx) = tx else {
            tracing::debug!("clipboard writer not configured; skipping write-back");
            return Ok(());
        };
        write_clipboard(&tx, text).await
    }

    async fn read_active_index(&self) -> zbus::fdo::Result<u32> {
        Ok(self
            .state
            .lock()
            .await
            .session
            .active_index()
            .active_index() as u32)
    }

    async fn history_count(&self) -> zbus::fdo::Result<u32> {
        Ok(self.state.lock().await.history().len() as u32)
    }
}