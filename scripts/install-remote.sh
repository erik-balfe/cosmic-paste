#!/usr/bin/env bash
# One-line installer: curl -fsSL …/install-remote.sh | bash
# Downloads the latest GitHub release bundle and installs for the current user.
set -euo pipefail

REPO="${COSMIC_PASTE_REPO:-erik-balfe/cosmic-paste}"
VERSION="${COSMIC_PASTE_VERSION:-}"
INSTALL_URL="${COSMIC_PASTE_INSTALL_URL:-https://raw.githubusercontent.com/${REPO}/master/scripts/install-remote.sh}"
TMP="${TMPDIR:-/tmp}/cosmic-paste-install.$$"
LIB_URL="https://raw.githubusercontent.com/${REPO}/master/scripts/lib/install-common.sh"

cleanup() { rm -rf "${TMP}"; }
trap cleanup EXIT

mkdir -p "${TMP}"
# shellcheck source=/dev/null
source <(curl -fsSL "${LIB_URL}")

echo "==> cosmic-paste installer (${REPO})"

if [[ "$(uname -s)" != "Linux" ]]; then
    echo "error: Linux only" >&2
    exit 1
fi

if [[ "$(uname -m)" != "x86_64" ]]; then
    echo "error: prebuilt releases are linux x86_64 only (build from source on other arches)" >&2
    exit 1
fi

cosmic_paste_install_runtime_deps

if [[ -z "${VERSION}" ]]; then
    echo "==> Resolving latest release..."
    VERSION="$(
        curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
            | grep -m1 '"tag_name"' \
            | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/'
    )"
    if [[ -z "${VERSION}" || "${VERSION}" == "null" ]]; then
        echo "error: no GitHub release found. Publish a release or set COSMIC_PASTE_VERSION=v0.1.0" >&2
        exit 1
    fi
fi

# Tag may be v0.1.0; asset name uses version without extra prefix handling
VER_NUM="${VERSION#v}"
ASSET="cosmic-paste-${VER_NUM}-linux-x86_64.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"

echo "==> Downloading ${VERSION} (${ASSET})..."
if ! curl -fsSL -o "${TMP}/${ASSET}" "${DOWNLOAD_URL}"; then
    echo "error: failed to download ${DOWNLOAD_URL}" >&2
    echo "       Check that release ${VERSION} exists and asset ${ASSET} is attached." >&2
    exit 1
fi

echo "==> Verifying checksum (if published)..."
if curl -fsSL -o "${TMP}/${ASSET}.sha256" "${DOWNLOAD_URL}.sha256" 2>/dev/null; then
    (cd "${TMP}" && sha256sum -c "${ASSET}.sha256")
else
    echo "    (no .sha256 on release — skipping verify)"
fi

echo "==> Extracting..."
tar -xzf "${TMP}/${ASSET}" -C "${TMP}"
BUNDLE_DIR="${TMP}/cosmic-paste-${VER_NUM}-linux-x86_64"
if [[ ! -d "${BUNDLE_DIR}" ]]; then
    echo "error: unexpected tarball layout" >&2
    exit 1
fi

bash "${BUNDLE_DIR}/scripts/install-release.sh"