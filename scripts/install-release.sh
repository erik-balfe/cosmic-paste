#!/usr/bin/env bash
# Install cosmic-paste from a release bundle (prebuilt binaries).
# Run from inside the extracted tarball or: scripts/install-release.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BINDIR="${HOME}/.local/bin"

# shellcheck source=scripts/lib/install-common.sh
source "${SCRIPT_DIR}/lib/install-common.sh"

echo "==> cosmic-paste release install"
cosmic_paste_install_runtime_deps

echo "==> Installing to ${BINDIR}..."
cosmic_paste_install_files "${ROOT}" "${BINDIR}" "${ROOT}/bin"
cosmic_paste_enable_daemon

VERSION="$(cosmic_paste_read_version "${ROOT}")"
cosmic_paste_print_finish "${VERSION}"