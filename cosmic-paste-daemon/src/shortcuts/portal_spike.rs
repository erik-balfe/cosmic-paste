//! GlobalShortcuts portal bindings (prev / next / show-history).

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use ashpd::desktop::ResponseError;
use ashpd::Error as AshpdError;
use cosmic_paste_core::dbus::client::CosmicPasteProxy;
use cosmic_paste_core::dbus::state::SharedDaemonState;
use cosmic_paste_core::{BUS_NAME, OBJECT_PATH};
use futures_util::StreamExt;
use tokio::sync::mpsc;

use crate::signals::DaemonSignal;

pub const SHOW_HISTORY_ID: &str = "show-history";
pub const SELECT_PREVIOUS_ID: &str = "select-previous";
pub const SELECT_NEXT_ID: &str = "select-next";

struct Binding {
    id: &'static str,
    description: &'static str,
    accel: String,
}

async fn set_portal_available(state: &SharedDaemonState, available: bool) {
    state.lock().await.portal_shortcuts_available = available;
}

fn is_user_denied(err: &AshpdError) -> bool {
    matches!(err, AshpdError::Response(ResponseError::Cancelled))
}

fn collect_bindings(
    show_history: &str,
    select_previous: &str,
    select_next: &str,
) -> Vec<Binding> {
    [
        Binding {
            id: SHOW_HISTORY_ID,
            description: "Open cosmic-paste history popup",
            accel: show_history.to_owned(),
        },
        Binding {
            id: SELECT_PREVIOUS_ID,
            description: "Select newer clipboard item",
            accel: select_previous.to_owned(),
        },
        Binding {
            id: SELECT_NEXT_ID,
            description: "Select older clipboard item",
            accel: select_next.to_owned(),
        },
    ]
    .into_iter()
    .filter(|binding| !binding.accel.is_empty())
    .collect()
}

async fn queue_show_history(signal_tx: &mpsc::Sender<DaemonSignal>) {
    if let Err(err) = signal_tx.send(DaemonSignal::ShowHistory).await {
        tracing::warn!("failed to queue ShowHistory signal: {err}");
    }
    cosmic_paste_core::show_history_trigger::signal();
    cosmic_paste_core::dbus::applet_activation::activate_show_history().await;
}

async fn select_at_offset(offset: i32) {
    let Ok(conn) = zbus::Connection::session().await else {
        return;
    };
    let proxy = match CosmicPasteProxy::builder(&conn).destination(BUS_NAME) {
        Ok(builder) => match builder.path(OBJECT_PATH) {
            Ok(builder) => match builder.build().await {
                Ok(proxy) => proxy,
                Err(err) => {
                    tracing::debug!("portal shortcut proxy build failed: {err}");
                    return;
                }
            },
            Err(err) => {
                tracing::debug!("portal shortcut proxy path failed: {err}");
                return;
            }
        },
        Err(err) => {
            tracing::debug!("portal shortcut proxy destination failed: {err}");
            return;
        }
    };
    if let Err(err) = proxy.select_at_offset(offset).await {
        tracing::debug!("portal select_at_offset({offset}) failed: {err}");
    }
}

async fn dispatch_shortcut(id: &str, signal_tx: &mpsc::Sender<DaemonSignal>) {
    match id {
        SHOW_HISTORY_ID => queue_show_history(signal_tx).await,
        SELECT_PREVIOUS_ID => select_at_offset(-1).await,
        SELECT_NEXT_ID => select_at_offset(1).await,
        other => tracing::debug!("ignored unknown portal shortcut: {other}"),
    }
}

/// Register configured shortcuts and listen for portal `Activated` events.
pub async fn run_portal_spike(
    state: SharedDaemonState,
    show_history_accel: String,
    select_previous_accel: String,
    select_next_accel: String,
    signal_tx: mpsc::Sender<DaemonSignal>,
) {
    let bindings = collect_bindings(
        &show_history_accel,
        &select_previous_accel,
        &select_next_accel,
    );
    if bindings.is_empty() {
        tracing::info!("all global shortcuts disabled (empty accelerators)");
        set_portal_available(&state, false).await;
        return;
    }

    let proxy = match GlobalShortcuts::new().await {
        Ok(proxy) => proxy,
        Err(err) => {
            tracing::warn!("GlobalShortcuts portal unavailable: {err}");
            set_portal_available(&state, false).await;
            return;
        }
    };

    let session = match proxy.create_session().await {
        Ok(session) => session,
        Err(err) => {
            tracing::warn!("failed to create GlobalShortcuts session: {err}");
            set_portal_available(&state, false).await;
            return;
        }
    };

    let shortcuts: Vec<NewShortcut> = bindings
        .iter()
        .map(|binding| {
            NewShortcut::new(binding.id, binding.description)
                .preferred_trigger(Some(binding.accel.as_str()))
        })
        .collect();

    let bind_req = match proxy.bind_shortcuts(&session, &shortcuts, None).await {
        Ok(req) => req,
        Err(err) => {
            tracing::warn!("failed to request shortcut bind: {err}");
            set_portal_available(&state, false).await;
            return;
        }
    };

    match bind_req.response() {
        Ok(response) => {
            tracing::info!(
                count = response.shortcuts().len(),
                "GlobalShortcuts bind succeeded"
            );
            set_portal_available(&state, true).await;
        }
        Err(err) => {
            if is_user_denied(&err) {
                tracing::warn!(
                    "GlobalShortcuts permission denied; set PortalShortcutsAvailable=false"
                );
            } else {
                tracing::warn!("GlobalShortcuts bind failed: {err}");
            }
            set_portal_available(&state, false).await;
            return;
        }
    }

    let mut activated = match proxy.receive_activated().await {
        Ok(stream) => stream,
        Err(err) => {
            tracing::warn!("failed to subscribe to GlobalShortcuts Activated: {err}");
            set_portal_available(&state, false).await;
            return;
        }
    };

    while let Some(activation) = activated.next().await {
        let id = activation.shortcut_id();
        tracing::info!(id, "GlobalShortcuts Activated");
        dispatch_shortcut(id, &signal_tx).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_cancelled_response_to_permission_denied() {
        assert!(is_user_denied(&AshpdError::Response(ResponseError::Cancelled)));
        assert!(!is_user_denied(&AshpdError::Response(ResponseError::Other)));
    }

    #[test]
    fn skips_empty_accelerators() {
        let bindings = collect_bindings("", "<Ctrl>F11", "");
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].id, SELECT_PREVIOUS_ID);
    }
}