#!/usr/bin/env bash
# One-command user install for cosmic-paste.
# Builds release binaries, installs to ~/.local/bin, enables the user daemon.
# Panel applet: add once via Settings → Desktop → Panel → Applets → COSMIC Paste.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINDIR="${HOME}/.local/bin"
RELEASE="${ROOT}/target/release"

need() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "error: required command not found: $1" >&2
        exit 1
    }
}

need cargo
need pkg-config
need systemctl

if ! pkg-config --exists xkbcommon 2>/dev/null; then
    echo "error: xkbcommon development files required (libxkbcommon-devel on Fedora)" >&2
    exit 1
fi

echo "==> Building release binaries..."
export PKG_CONFIG_PATH="${PKG_CONFIG_PATH:-}:/usr/lib64/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig"
(
    cd "${ROOT}"
    cargo build --release -p cosmic-paste-daemon -p cosmic-paste-cli -p cosmic-paste-applet
)

echo "==> Installing to ${BINDIR}..."
mkdir -p \
    "${HOME}/.config/systemd/user" \
    "${HOME}/.local/share/dbus-1/services" \
    "${HOME}/.local/share/applications" \
    "${HOME}/.local/share/icons/hicolor/scalable/apps" \
    "${BINDIR}"

install -m 0755 \
    "${RELEASE}/cosmic-paste-daemon" \
    "${RELEASE}/cosmic-paste" \
    "${RELEASE}/cosmic-paste-applet" \
    "${ROOT}/scripts/cosmic-paste-show-history" \
    "${BINDIR}/"

sed "s|@bindir@|${BINDIR}|g" "${ROOT}/data/systemd/com.system76.CosmicPaste.service" \
    > "${HOME}/.config/systemd/user/com.system76.CosmicPaste.service"

sed "s|@bindir@|${BINDIR}|g" "${ROOT}/data/dbus/org.system76.CosmicPaste.service" \
    > "${HOME}/.local/share/dbus-1/services/org.system76.CosmicPaste.service"

sed "s|@bindir@|${BINDIR}|g" "${ROOT}/data/com.system76.CosmicPaste.Applet.desktop" \
    > "${HOME}/.local/share/applications/com.system76.CosmicPaste.Applet.desktop"

install -m 0644 \
    "${ROOT}/cosmic-paste-applet/icons/paste-symbolic.svg" \
    "${HOME}/.local/share/icons/hicolor/scalable/apps/com.system76.CosmicPaste.Applet-symbolic.svg"

rm -f \
    "${HOME}/.local/share/dbus-1/services/com.system76.CosmicPaste.service" \
    "${HOME}/.local/share/dbus-1/services/com.system76.CosmicPaste.Applet.service"

pkill -x cosmic-paste-applet 2>/dev/null || true
systemctl --user daemon-reload
systemctl --user enable --now com.system76.CosmicPaste.service

echo ""
echo "Installed cosmic-paste $(grep '^version' "${ROOT}/Cargo.toml" | head -1 | awk '{print $3}' | tr -d '"')"
echo ""
echo "Clipboard daemon is running (com.system76.CosmicPaste.service)."
echo ""
echo "One manual step — add the panel applet:"
echo "  Settings → Desktop → Panel → Applets → End segment → COSMIC Paste"
echo "  If the icon does not appear: killall cosmic-panel"
echo ""
echo "Default shortcuts: Ctrl+F9 newer, Ctrl+F10 older, Ctrl+F11 popup."
echo "Check portal: busctl --user get-property org.system76.CosmicPaste /org/system76/CosmicPaste org.system76.CosmicPaste2 PortalShortcutsAvailable"
echo "If false (common on COSMIC), add custom Spawn shortcuts:"
echo "  ${ROOT}/data/examples/cosmic-custom-shortcuts.ron"
echo "  Merge into ~/.config/cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom"
echo ""
echo "CLI: cosmic-paste history | prev | next | show-history"