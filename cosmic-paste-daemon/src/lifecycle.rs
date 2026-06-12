use cosmic_paste_core::dbus::lifecycle::ShutdownReason;
use cosmic_paste_core::dbus::state::SharedDaemonState;
use tokio::sync::watch;
use tracing::{info, warn};

pub use cosmic_paste_core::dbus::lifecycle::LifecycleHandle;

/// Initialize tracing: journald when available (systemd), stderr fmt otherwise.
pub fn init_logging() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,cosmic_paste_daemon=debug"));

    if let Ok(journal_layer) = tracing_journald::layer() {
        tracing_subscriber::registry()
            .with(journal_layer)
            .with(filter)
            .init();
        return;
    }

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false))
        .with(filter)
        .init();
}

pub async fn wait_for_shutdown(mut lifecycle_rx: watch::Receiver<ShutdownReason>) -> ShutdownReason {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => ShutdownReason::None,
        reason = wait_lifecycle(&mut lifecycle_rx) => reason,
        _ = sigterm() => ShutdownReason::None,
    }
}

async fn wait_lifecycle(rx: &mut watch::Receiver<ShutdownReason>) -> ShutdownReason {
    loop {
        if rx.changed().await.is_err() {
            return ShutdownReason::None;
        }
        let reason = *rx.borrow();
        if reason != ShutdownReason::None {
            return reason;
        }
    }
}

#[cfg(unix)]
async fn sigterm() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut stream = signal(SignalKind::terminate()).expect("register SIGTERM handler");
    stream.recv().await;
}

#[cfg(not(unix))]
async fn sigterm() {
    std::future::pending::<()>().await;
}

pub async fn flush_state(state: &SharedDaemonState) {
    let guard = state.lock().await;
    if let Err(err) = guard.persist() {
        warn!("failed to flush daemon state on shutdown: {err}");
    } else {
        info!("daemon state flushed");
    }
}

#[cfg(unix)]
pub fn reexec_or_exit(code: i32) -> ! {
    use std::os::unix::process::CommandExt;

    let program = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            warn!("reexecute failed to resolve executable path: {err}");
            std::process::exit(code);
        }
    };

    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let err = std::process::Command::new(program).args(&args[1..]).exec();
    warn!("reexecute exec failed: {err}");
    std::process::exit(code);
}

#[cfg(not(unix))]
pub fn reexec_or_exit(code: i32) -> ! {
    std::process::exit(code);
}