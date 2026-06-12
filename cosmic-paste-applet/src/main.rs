mod app;
mod dbus_sub;
mod icons;

fn main() -> cosmic::iced::Result {
    // cosmic-panel passes WAYLAND_SOCKET when embedding applets in the tray.
    // Without it (apps menu, raw dbus activation, terminal), libcosmic falls back
    // to a floating window — not a panel applet.
    if std::env::var_os("WAYLAND_SOCKET").is_none() {
        eprintln!(
            "cosmic-paste-applet: must run from the COSMIC panel.\n\
             Add it in Settings → Desktop → Panel → Applets → End segment,\n\
             then restart cosmic-panel. CLI: cosmic-paste history"
        );
        return Ok(());
    }

    let open_popup = std::env::args().any(|arg| arg == "--show-history");
    cosmic::applet::run::<app::App>(app::Flags { open_popup })
}