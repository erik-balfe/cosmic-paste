#!/usr/bin/env bash
# Install from git checkout (builds release binaries locally).
# Panel applet: add once via Settings → Desktop → Panel → Applets → COSMIC Paste.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINDIR="${HOME}/.local/bin"
RELEASE="${ROOT}/target/release"
LIB="${ROOT}/scripts/lib/install-common.sh"

# shellcheck source=scripts/lib/install-common.sh
source "${LIB}"

cosmic_paste_need cargo
cosmic_paste_need pkg-config

if ! pkg-config --exists xkbcommon 2>/dev/null; then
    echo "==> Build dependencies missing — installing..."
    cosmic_paste_install_build_deps
fi

echo "==> Building release binaries..."
export PKG_CONFIG_PATH="${PKG_CONFIG_PATH:-}:/usr/lib64/pkgconfig:/usr/lib/pkgconfig:/usr/share/pkgconfig"
(
    cd "${ROOT}"
    cargo build --release -p cosmic-paste-daemon -p cosmic-paste-cli -p cosmic-paste-applet
)

echo "==> Installing to ${BINDIR}..."
cosmic_paste_install_files "${ROOT}" "${BINDIR}" "${RELEASE}"
cosmic_paste_enable_daemon

VERSION="$(cosmic_paste_read_version "${ROOT}")"
cosmic_paste_print_finish "${VERSION}"