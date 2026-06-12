export NAME := 'cosmic-paste'

default: test

clean:
    cargo clean

build:
    cargo build --workspace

build-release:
    cargo build --release --workspace

test:
    cargo test --workspace

check:
    cargo clippy --workspace --all-targets -- -D warnings

run-daemon:
    cargo run -p cosmic-paste-daemon

run-applet:
    cargo run -p cosmic-paste-applet

# User-session install for DBus/systemd activation testing (does not require root).
install-user:
    #!/usr/bin/env bash
    set -euo pipefail
    root="{{justfile_directory()}}"
    bindir="${root}/target/debug"
    cargo build -p cosmic-paste-daemon
    mkdir -p "${HOME}/.config/systemd/user" "${HOME}/.local/share/dbus-1/services"
    sed "s|@bindir@|${bindir}|g" "${root}/data/systemd/com.system76.CosmicPaste.service" \
        > "${HOME}/.config/systemd/user/com.system76.CosmicPaste.service"
    sed "s|@bindir@|${bindir}|g" "${root}/data/dbus/com.system76.CosmicPaste.service" \
        > "${HOME}/.local/share/dbus-1/services/com.system76.CosmicPaste.service"
    systemctl --user daemon-reload
    echo "Installed user units. Enable with: systemctl --user enable --now com.system76.CosmicPaste.service"

uninstall-user:
    systemctl --user disable --now com.system76.CosmicPaste.service 2>/dev/null || true
    rm -f "${HOME}/.config/systemd/user/com.system76.CosmicPaste.service"
    rm -f "${HOME}/.local/share/dbus-1/services/com.system76.CosmicPaste.service"
    systemctl --user daemon-reload