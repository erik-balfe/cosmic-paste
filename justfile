export NAME := 'cosmic-paste'

# libcosmic / smithay-client-toolkit needs xkbcommon.pc (libxkbcommon-devel on Fedora).
export PKG_CONFIG_PATH := '/usr/lib64/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig'

default: test

clean:
    cargo clean

build:
    cargo build --workspace

build-release:
    cargo build --release --workspace

test:
    cargo test --workspace

# Manual COSMIC session check: binds show-history then waits for Ctrl+Alt+H.
test-portal:
    cargo test -p cosmic-paste-daemon --test portal_shortcut_fire portal_shortcut_fire -- --ignored --nocapture

check:
    cargo clippy --workspace --all-targets -- -D warnings

run-daemon:
    cargo run -p cosmic-paste-daemon

run-applet:
    cargo run -p cosmic-paste-applet

run-cli *args:
    cargo run -p cosmic-paste-cli -- {{args}}

# User-session install for DBus/systemd activation testing (does not require root).
install-user:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! pkg-config --exists xkbcommon 2>/dev/null; then
        echo "error: xkbcommon not found via pkg-config (needed for cosmic-paste-applet)" >&2
        echo "  Fedora: sudo dnf install libxkbcommon-devel" >&2
        exit 1
    fi
    root="{{justfile_directory()}}"
    bindir="${root}/target/debug"
    cargo build -p cosmic-paste-daemon -p cosmic-paste-cli -p cosmic-paste-applet
    mkdir -p \
        "${HOME}/.config/systemd/user" \
        "${HOME}/.local/share/dbus-1/services" \
        "${HOME}/.local/share/applications" \
        "${HOME}/.local/share/icons/hicolor/scalable/apps" \
        "${HOME}/.local/bin"
    install -m 0755 \
        "${bindir}/cosmic-paste-daemon" \
        "${bindir}/cosmic-paste" \
        "${bindir}/cosmic-paste-applet" \
        "${HOME}/.local/bin/"
    sed "s|@bindir@|${bindir}|g" "${root}/data/systemd/com.system76.CosmicPaste.service" \
        > "${HOME}/.config/systemd/user/com.system76.CosmicPaste.service"
    sed "s|@bindir@|${bindir}|g" "${root}/data/dbus/org.system76.CosmicPaste.service" \
        > "${HOME}/.local/share/dbus-1/services/org.system76.CosmicPaste.service"
    rm -f "${HOME}/.local/share/dbus-1/services/com.system76.CosmicPaste.Applet.service"
    sed "s|@bindir@|${bindir}|g" "${root}/data/com.system76.CosmicPaste.Applet.desktop" \
        > "${HOME}/.local/share/applications/com.system76.CosmicPaste.Applet.desktop"
    pkill -x cosmic-paste-applet 2>/dev/null || true
    install -m 0644 \
        "${root}/cosmic-paste-applet/icons/paste-symbolic.svg" \
        "${HOME}/.local/share/icons/hicolor/scalable/apps/com.system76.CosmicPaste.Applet-symbolic.svg"
    rm -f "${HOME}/.local/share/dbus-1/services/com.system76.CosmicPaste.service"
    systemctl --user daemon-reload
    echo "Installed user units, desktop file, and settings icon."
    echo "  Daemon: systemctl --user enable --now com.system76.CosmicPaste.service"
    echo "  Panel tray (required — do NOT launch from the apps menu):"
    echo "    Settings → Desktop → Panel → Applets → End segment → COSMIC Paste"
    echo "    (Use just reset-applet-in-settings first if re-adding from Add applet drawer.)"
    echo "    Then: killall cosmic-panel   # session respawns it with the tray icon"
    echo "  CLI:    cosmic-paste history | select <uuid> | prev | next | add 'text'"

# Remove Cosmic Paste from panel/dock config so it reappears in Settings → Add applet.
reset-applet-in-settings:
    #!/usr/bin/env bash
    set -euo pipefail
    applet="com.system76.CosmicPaste.Applet"
    panel="${HOME}/.config/cosmic/com.system76.CosmicPanel.Panel/v1/plugins_wings"
    dock="${HOME}/.config/cosmic/com.system76.CosmicPanel.Dock/v1/plugins_center"
    pkill -f '/cosmic-paste-applet' 2>/dev/null || true
    remove_applet() {
        local file="$1"
        [[ -f "${file}" ]] || return 0
        if grep -q "\"${applet}\"" "${file}"; then
            grep -v "\"${applet}\"" "${file}" > "${file}.tmp"
            mv "${file}.tmp" "${file}"
            echo "updated ${file}"
        fi
    }
    remove_applet "${panel}"
    remove_applet "${dock}"
    echo "Reopen Settings → Panel → Applets → Add applet → COSMIC Paste (end segment only)."
    echo "Then: killall cosmic-panel"

# Print RON snippets for Settings → Keyboard → Custom shortcuts (Ctrl+F9–F12 example).
show-cosmic-shortcuts:
    @cat "{{justfile_directory()}}/data/examples/cosmic-custom-shortcuts.ron"
    @echo ""
    @echo "Add via COSMIC Settings → Keyboard → Custom shortcuts,"
    @echo "or merge into ~/.config/cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom"

uninstall-user:
    systemctl --user disable --now com.system76.CosmicPaste.service 2>/dev/null || true
    rm -f "${HOME}/.config/systemd/user/com.system76.CosmicPaste.service"
    rm -f "${HOME}/.local/share/dbus-1/services/org.system76.CosmicPaste.service"
    rm -f "${HOME}/.local/share/dbus-1/services/com.system76.CosmicPaste.service"
    rm -f "${HOME}/.local/share/dbus-1/services/com.system76.CosmicPaste.Applet.service"
    rm -f "${HOME}/.local/bin/cosmic-paste-daemon" \
        "${HOME}/.local/bin/cosmic-paste" \
        "${HOME}/.local/bin/cosmic-paste-applet"
    rm -f "${HOME}/.local/share/applications/com.system76.CosmicPaste.Applet.desktop"
    rm -f "${HOME}/.local/share/icons/hicolor/scalable/apps/com.system76.CosmicPaste.Applet-symbolic.svg"
    systemctl --user daemon-reload