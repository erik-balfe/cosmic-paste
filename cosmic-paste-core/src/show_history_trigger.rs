//! Event-driven show-history signal between daemon/CLI and the panel applet.

use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const DIR_NAME: &str = "cosmic-paste";
const FILE_NAME: &str = "show-history";
const SOCKET_NAME: &str = "show-history.sock";

fn runtime_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
}

/// Path watched by the panel applet (`$XDG_RUNTIME_DIR/cosmic-paste/show-history`).
pub fn trigger_path() -> Option<PathBuf> {
    runtime_dir().map(|runtime| runtime.join(DIR_NAME).join(FILE_NAME))
}

/// Unix datagram socket for instant delivery (more reliable than inotify alone).
pub fn socket_path() -> Option<PathBuf> {
    runtime_dir().map(|runtime| runtime.join(DIR_NAME).join(SOCKET_NAME))
}

/// Notify the running applet to open its popup.
pub fn signal() {
    let Some(path) = trigger_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string();
    let _ = std::fs::write(path, stamp);
    signal_socket();
}

fn signal_socket() {
    let Some(path) = socket_path() else {
        return;
    };
    if let Ok(sender) = std::os::unix::net::UnixDatagram::unbound() {
        let _ = sender.send_to(b"1", path);
    }
}

/// Bind the show-history socket (panel applet only).
pub fn bind_socket() -> io::Result<std::os::unix::net::UnixDatagram> {
    let path = socket_path().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR unavailable")
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&path);
    std::os::unix::net::UnixDatagram::bind(path)
}