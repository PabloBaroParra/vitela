#!/usr/bin/env bash
#
# One-shot version of the manual Linux packaging walkthrough: installs the
# system/build/packaging tooling this needs, fetches linuxdeploy,
# appimagetool and the pinned PDFium input, builds the release binary, and
# runs package-linux.sh. Idempotent — re-running skips whatever it already
# fetched or built.
#
#   bash scripts/bootstrap-linux-package.sh
#
# Requires Linux x86_64 (same requirement package-linux.sh and
# verify-linux-package.sh enforce) and sudo for the apt-get step.
set -euo pipefail

if [ "$(uname -s)" != Linux ] || [ "$(uname -m)" != x86_64 ]; then
    echo "bootstrap-linux-package: this packages Vitela for Linux x86_64; run it on that platform." >&2
    exit 1
fi

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly BUILD_ROOT="${PACKAGE_BUILD_ROOT:-$REPO_ROOT/build/linux}"
readonly TOOLS_DIR="$BUILD_ROOT/tools"
readonly LINUXDEPLOY_URL='https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage'
readonly APPIMAGETOOL_URL='https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage'
# Same chromium/7763 release package-linux.sh's own PDFIUM_SHA256_DEFAULT
# pins and verifies — this script only fetches the file, package-linux.sh
# is what actually checks it before trusting it.
readonly PDFIUM_URL='https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/7763/pdfium-linux-x64.tgz'

log() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }

log "Installing system packages (apt)"
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libgtk-4-dev dpkg-dev file binutils curl libfuse2

if ! command -v rustup >/dev/null 2>&1; then
    log "Installing rustup (stable)"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain stable
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
fi

mkdir -p "$TOOLS_DIR"

fetch_tool() {
    local url="$1" destination="$2"
    if [ ! -x "$destination" ]; then
        log "Downloading $(basename "$destination")"
        curl -sL "$url" -o "$destination"
        chmod +x "$destination"
    fi
}
fetch_tool "$LINUXDEPLOY_URL" "$TOOLS_DIR/linuxdeploy"
fetch_tool "$APPIMAGETOOL_URL" "$TOOLS_DIR/appimagetool"

pdfium_archive="$TOOLS_DIR/pdfium-linux-x64.tgz"
if [ ! -f "$pdfium_archive" ]; then
    log "Downloading the PDFium packaging input (package-linux.sh verifies its checksum)"
    curl -sL "$PDFIUM_URL" -o "$pdfium_archive"
fi

log "Building linux-gtk (release)"
( cd "$REPO_ROOT" && cargo build --release -p linux-gtk )

log "Packaging .deb and .AppImage"
LINUXDEPLOY="$TOOLS_DIR/linuxdeploy" \
APPIMAGETOOL="$TOOLS_DIR/appimagetool" \
PDFIUM_ARCHIVE="$pdfium_archive" \
    "$REPO_ROOT/scripts/package-linux.sh"

log "Done — packages in $BUILD_ROOT/packages"
