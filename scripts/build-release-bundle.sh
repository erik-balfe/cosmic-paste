#!/usr/bin/env bash
# Build cosmic-paste release tarball for linux x86_64 (run after cargo build --release).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE="${ROOT}/target/release"
VERSION="$(grep '^version' "${ROOT}/Cargo.toml" | head -1 | awk '{print $3}' | tr -d '"')"
BUNDLE="cosmic-paste-${VERSION}-linux-x86_64"
STAGING="${ROOT}/target/${BUNDLE}"
ARCHIVE="${ROOT}/target/${BUNDLE}.tar.gz"

for bin in cosmic-paste-daemon cosmic-paste cosmic-paste-applet; do
    if [[ ! -x "${RELEASE}/${bin}" ]]; then
        echo "error: missing ${RELEASE}/${bin} — run: cargo build --release -p cosmic-paste-daemon -p cosmic-paste-cli -p cosmic-paste-applet" >&2
        exit 1
    fi
done

rm -rf "${STAGING}"
mkdir -p "${STAGING}/bin" "${STAGING}/data" "${STAGING}/share/icons" "${STAGING}/scripts"

cp "${RELEASE}/cosmic-paste-daemon" "${RELEASE}/cosmic-paste" "${RELEASE}/cosmic-paste-applet" "${STAGING}/bin/"
cp "${ROOT}/scripts/cosmic-paste-show-history" "${STAGING}/bin/"
chmod +x "${STAGING}/bin/"*

cp -a "${ROOT}/data/." "${STAGING}/data/"
cp "${ROOT}/cosmic-paste-applet/icons/paste-symbolic.svg" \
    "${STAGING}/share/icons/com.system76.CosmicPaste.Applet-symbolic.svg"

mkdir -p "${STAGING}/scripts/lib"
cp "${ROOT}/scripts/install-release.sh" "${STAGING}/scripts/"
cp "${ROOT}/scripts/lib/install-common.sh" "${STAGING}/scripts/lib/"
chmod +x "${STAGING}/scripts/install-release.sh"

echo "${VERSION}" > "${STAGING}/VERSION"

tar -C "${ROOT}/target" -czf "${ARCHIVE}" "${BUNDLE}"
sha256sum "${ARCHIVE}" > "${ARCHIVE}.sha256"

echo "Created ${ARCHIVE}"
echo "       ${ARCHIVE}.sha256"