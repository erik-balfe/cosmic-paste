use cosmic_paste_core::dbus::service::CosmicPasteService;
use cosmic_paste_core::OBJECT_PATH;
use tokio::sync::mpsc;
use zbus::object_server::SignalEmitter;

pub enum DaemonSignal {
    Update {
        action: &'static str,
        target: String,
        index: u32,
    },
    ActiveIndexChanged {
        index: u32,
        count: u32,
    },
    ShowHistory,
}

pub async fn run_signal_emitter(
    connection: zbus::Connection,
    mut rx: mpsc::Receiver<DaemonSignal>,
) {
    while let Some(signal) = rx.recv().await {
        let Ok(emitter) = SignalEmitter::new(&connection, OBJECT_PATH) else {
            tracing::warn!("failed to create DBus signal emitter");
            continue;
        };

        match signal {
            DaemonSignal::Update {
                action,
                target,
                index,
            } => {
                if let Err(err) = CosmicPasteService::emit_history_update(
                    emitter.clone(),
                    action,
                    &target,
                    index,
                )
                .await
                {
                    tracing::warn!("failed to emit Update signal: {err}");
                }
            }
            DaemonSignal::ActiveIndexChanged { index, count } => {
                if let Err(err) =
                    CosmicPasteService::emit_active_index(emitter, index, count).await
                {
                    tracing::warn!("failed to emit ActiveIndexChanged signal: {err}");
                }
            }
            DaemonSignal::ShowHistory => {
                if let Err(err) = CosmicPasteService::emit_show_history(emitter).await {
                    tracing::warn!("failed to emit ShowHistory signal: {err}");
                }
                cosmic_paste_core::show_history_trigger::signal();
                cosmic_paste_core::dbus::applet_activation::activate_show_history().await;
            }
        }
    }
}