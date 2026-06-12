//! PR 7a: ashpd GlobalShortcuts proof-of-life (single `show-history` binding).

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use ashpd::desktop::ResponseError;
use ashpd::Error as AshpdError;
use cosmic_paste_core::dbus::state::SharedDaemonState;
use futures_util::StreamExt;
use tokio::sync::mpsc;

use crate::signals::DaemonSignal;

/// Action ID for the spike shortcut (becomes `show-history` in PR 7).
pub const SHOW_HISTORY_ID: &str = "show-history";

async fn set_portal_available(state: &SharedDaemonState, available: bool) {
    state.lock().await.portal_shortcuts_available = available;
}

fn is_user_denied(err: &AshpdError) -> bool {
    matches!(err, AshpdError::Response(ResponseError::Cancelled))
}

/// Register `show-history` and listen for portal `Activated` events.
pub async fn run_portal_spike(
    state: SharedDaemonState,
    accel: String,
    signal_tx: mpsc::Sender<DaemonSignal>,
) {
    if accel.is_empty() {
        tracing::info!("show-history shortcut disabled (empty accelerator)");
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

    let shortcut = NewShortcut::new(
        SHOW_HISTORY_ID,
        "Show cosmic-paste history (portal spike)",
    )
    .preferred_trigger(Some(accel.as_str()));

    let bind_req = match proxy.bind_shortcuts(&session, &[shortcut], None).await {
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
                id = SHOW_HISTORY_ID,
                accel = %accel,
                "GlobalShortcuts bind succeeded (PR7a spike)"
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
        if id == SHOW_HISTORY_ID {
            let present = state.lock().await.applet_present;
            if present {
                if let Err(err) = signal_tx.send(DaemonSignal::ShowHistory).await {
                    tracing::warn!("failed to queue ShowHistory signal: {err}");
                }
            } else {
                tracing::info!(
                    "show-history shortcut fired; add COSMIC Paste to the panel or use `cosmic-paste history`"
                );
            }
        }
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
}