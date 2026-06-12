mod ingest;
mod lifecycle;
mod monitor;
mod signals;

use cosmic_paste_core::dbus::lifecycle::ShutdownReason;
use cosmic_paste_core::dbus::state::DaemonState;
use cosmic_paste_core::{BUS_NAME, OBJECT_PATH};
use lifecycle::{flush_state, init_logging, reexec_or_exit, wait_for_shutdown, LifecycleHandle};
use tokio::sync::mpsc;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    init_logging();

    let mut daemon = match DaemonState::load_default() {
        Ok(state) => state,
        Err(err) => {
            error!("failed to load daemon state, starting in-memory: {err}");
            DaemonState::new_in_memory()
        }
    };

    let (clipboard_write_tx, clipboard_write_rx) = std::sync::mpsc::channel();
    daemon.set_clipboard_writer(clipboard_write_tx);

    let (lifecycle, lifecycle_rx) = LifecycleHandle::pair();
    let service = daemon.service(lifecycle);
    let shared = service.shared_state();

    let (clipboard_tx, clipboard_rx) = mpsc::channel(64);
    let (signal_tx, signal_rx) = mpsc::channel(64);
    let monitor = monitor::ClipboardMonitor::new(monitor::MonitorConfig::default());
    let monitor_handle = monitor.spawn(clipboard_tx, clipboard_write_rx);
    tokio::spawn(ingest::run_ingest_loop(
        clipboard_rx,
        shared.clone(),
        Some(signal_tx),
    ));

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

    let signal_connection = connection.clone();
    let signal_task = tokio::spawn(signals::run_signal_emitter(signal_connection, signal_rx));

    let shutdown = wait_for_shutdown(lifecycle_rx).await;
    info!(?shutdown, "cosmic-paste daemon shutting down");

    flush_state(&shared).await;
    drop(connection);
    signal_task.abort();
    monitor_handle.join();

    if shutdown == ShutdownReason::Reexecute {
        info!("reexecuting cosmic-paste-daemon");
        reexec_or_exit(0);
    }
}