export NAME := 'cosmic-paste'

# libcosmic / smithay-client-toolkit needs xkbcommon.pc (libxkbcommon-devel on Fedora).
export PKG_CONFIG_PATH := '/usr/lib64/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig'

default: test

# Install from git checkout (builds locally).
install:
    "{{justfile_directory()}}/scripts/install.sh"

# Build release tarball (after cargo build --release).
release-bundle:
    chmod +x "{{justfile_directory()}}/scripts/build-release-bundle.sh"
    "{{justfile_directory()}}/scripts/build-release-bundle.sh"

clean:
    cargo clean

build:
    cargo build --workspace

build-release:
    cargo build --release --workspace

test:
    cargo test --workspace

# Manual COSMIC session check: binds show-history then waits for Ctrl+F11.
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

# User-session install (debug build — faster iteration during development).
install-user:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! pkg-config --exists xkbcommon 2>/dev/null; then
        echo "error: xkbcommon not found via pkg-config (needed for cosmic-paste-applet)" >&2
        echo "  Fedora: sudo dnf install libxkbcommon-devel" >&2
        exit 1
    fi
    root="{{justfile_directory()}}"
    builddir="${root}/target/debug"
    bindir="${HOME}/.local/bin"
    cargo build -p cosmic-paste-daemon -p cosmic-paste-cli -p cosmic-paste-applet
    mkdir -p \
        "${HOME}/.config/systemd/user" \
        "${HOME}/.local/share/dbus-1/services" \
        "${HOME}/.local/share/applications" \
        "${HOME}/.local/share/icons/hicolor/scalable/apps" \
        "${bindir}"
    install -m 0755 \
        "${builddir}/cosmic-paste-daemon" \
        "${builddir}/cosmic-paste" \
        "${builddir}/cosmic-paste-applet" \
        "${root}/scripts/cosmic-paste-show-history" \
        "${bindir}/"
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

# Optimized release install for day-to-day use (faster applet popup).
install-user-release:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! pkg-config --exists xkbcommon 2>/dev/null; then
        echo "error: xkbcommon not found via pkg-config (needed for cosmic-paste-applet)" >&2
        echo "  Fedora: sudo dnf install libxkbcommon-devel" >&2
        exit 1
    fi
    root="{{justfile_directory()}}"
    builddir="${root}/target/release"
    bindir="${HOME}/.local/bin"
    cargo build --release -p cosmic-paste-daemon -p cosmic-paste-cli -p cosmic-paste-applet
    mkdir -p \
        "${HOME}/.config/systemd/user" \
        "${HOME}/.local/share/dbus-1/services" \
        "${HOME}/.local/share/applications" \
        "${HOME}/.local/share/icons/hicolor/scalable/apps" \
        "${bindir}"
    install -m 0755 \
        "${builddir}/cosmic-paste-daemon" \
        "${builddir}/cosmic-paste" \
        "${builddir}/cosmic-paste-applet" \
        "${root}/scripts/cosmic-paste-show-history" \
        "${bindir}/"
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
    echo "Installed release binaries to ${HOME}/.local/bin"
    echo "  Then: systemctl --user restart com.system76.CosmicPaste.service && killall cosmic-panel"

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

# Print RON snippets for Settings → Keyboard → Custom shortcuts (Ctrl+F9–F11).
show-cosmic-shortcuts:
    @cat "{{justfile_directory()}}/data/examples/cosmic-custom-shortcuts.ron"
    @echo ""
    @echo "Add via COSMIC Settings → Keyboard → Custom shortcuts,"
    @echo "or merge into ~/.config/cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom"

# Verify SelectAtIndex / clipboard write-back (same path as tray popup clicks).
test-clipboard-select:
    #!/usr/bin/env bash
    set -euo pipefail
    command -v wl-paste >/dev/null || { echo "wl-paste required" >&2; exit 1; }
    token="cosmic-paste-select-test-$(date +%s)"
    cosmic-paste add "${token}" >/dev/null
    dbus_text() {
        busctl --user call org.system76.CosmicPaste /org/system76/CosmicPaste \
            org.system76.CosmicPaste2 GetElementAtIndex u "$1" 2>&1 | awk -F'"' '{ print $4 }'
    }
    expected="${token}"
    busctl --user call org.system76.CosmicPaste /org/system76/CosmicPaste \
        org.system76.CosmicPaste2 SelectAtIndex u 0 >/dev/null
    sleep 0.35
    actual=$(wl-paste -n 2>/dev/null || true)
    if [[ "${actual}" != "${expected}" ]]; then
        echo "FAIL index 0: expected '${expected}', clipboard='${actual}'" >&2
        exit 1
    fi
    busctl --user call org.system76.CosmicPaste /org/system76/CosmicPaste \
        org.system76.CosmicPaste2 SelectAtIndex u 1 >/dev/null
    sleep 0.35
    actual=$(wl-paste -n 2>/dev/null || true)
    if [[ -z "${actual}" || "${actual}" == "Nothing is copied" ]]; then
        echo "FAIL index 1: clipboard empty after SelectAtIndex" >&2
        exit 1
    fi
    size=$(busctl --user call org.system76.CosmicPaste /org/system76/CosmicPaste \
        org.system76.CosmicPaste2 GetHistory 2>&1 | awk '{ print $2 }')
    if [[ "${size}" -ge 3 ]]; then
        expected=$(dbus_text 2)
        busctl --user call org.system76.CosmicPaste /org/system76/CosmicPaste \
            org.system76.CosmicPaste2 SelectAtIndex u 2 >/dev/null
        sleep 0.35
        actual=$(wl-paste -n 2>/dev/null || true)
        if [[ "${actual}" != "${expected}" ]]; then
            echo "FAIL index 2: expected '${expected}', clipboard='${actual}'" >&2
            exit 1
        fi
        active=$(busctl --user get-property org.system76.CosmicPaste /org/system76/CosmicPaste \
            org.system76.CosmicPaste2 ActiveIndex 2>&1 | awk '{ print $2 }')
        if [[ "${active}" != "2" ]]; then
            echo "FAIL active index: expected 2, got ${active}" >&2
            exit 1
        fi
        echo "OK: SelectAtIndex updates clipboard (index 0 and index 2)"
    else
        echo "OK: SelectAtIndex updates clipboard (index 0)"
    fi

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