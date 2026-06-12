mod ingest;
mod monitor;

use cosmic_paste_core::dbus::state::DaemonState;
use cosmic_paste_core::{BUS_NAME, OBJECT_PATH};
use tokio::sync::mpsc;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let daemon = match DaemonState::load_default() {
        Ok(state) => state,
        Err(err) => {
            error!("failed to load daemon state, starting in-memory: {err}");
            DaemonState::new_in_memory()
        }
    };

    let service = daemon.service();
    let shared = service.shared_state();

    let (clipboard_tx, clipboard_rx) = mpsc::channel(64);
    let monitor = monitor::ClipboardMonitor::new(monitor::MonitorConfig::default());
    let monitor_handle = monitor.spawn(clipboard_tx);
    tokio::spawn(ingest::run_ingest_loop(clipboard_rx, shared));

    let connection = match zbus::connection::Builder::session() {
        Ok(builder) => match builder
            .name(BUS_NAME)
            .and_then(|builder| builder.serve_at(OBJECT_PATH, service))
        {
            Ok(builder) => builder.build().await,
            Err(err) => Err(err),
        },
        Err(err) => Err(err),
    };

    let connection = match connection {
        Ok(connection) => connection,
        Err(err) => {
            error!("failed to start DBus service: {err}");
            std::process::exit(1);
        }
    };

    info!("cosmic-paste daemon ready on {BUS_NAME}{OBJECT_PATH}");

    if let Err(err) = tokio::signal::ctrl_c().await {
        error!("failed to listen for shutdown signal: {err}");
        std::process::exit(1);
    }

    drop(connection);
    monitor_handle.join();
    info!("cosmic-paste daemon shutting down");
}