#!/usr/bin/env bash
#
# Idempotent provisioning for the Linux GTK4 shell (apps/linux-gtk) under
# WSL2 + WSLg. Run it once on a fresh Ubuntu, and re-run any time to update:
# every step is safe to repeat, and bumping a pinned version below then
# re-running is the whole "update" workflow.
#
#   bash scripts/linux-dev-setup.sh
#
set -euo pipefail

# --- Pinned versions (bump + re-run to update) -------------------------------
# pdfium: must match pdfium-render's enabled `pdfium_XXXX` Cargo feature
# (currently pdfium_7763 — see core/pdf-render/Cargo.toml). Non-V8 build, same
# release the CI workflow pins (see .github/workflows/core.yml).
PDFIUM_RELEASE="7763"
RUST_TOOLCHAIN="stable"

# Resolve the repo root from this script's own location, so it works no matter
# the current directory.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PDFIUM_DIR="$REPO_ROOT/core/pdf-render/vendor/pdfium"
PDFIUM_LIB="$PDFIUM_DIR/lib/libpdfium.so"
PDFIUM_MARKER="$PDFIUM_DIR/.release"

log() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }

# --- Guard: this provisions a Linux environment (WSL counts) -----------------
if [ "$(uname -s)" != "Linux" ]; then
  echo "This script provisions a Linux environment; run it inside WSL2 Ubuntu." >&2
  exit 1
fi

# --- System packages: GTK4 + build toolchain ---------------------------------
log "Installing GTK4 dev libraries and build tooling (apt)"
sudo apt-get update
sudo apt-get install -y libgtk-4-dev build-essential pkg-config curl

# --- Rust toolchain ----------------------------------------------------------
if ! command -v rustup >/dev/null 2>&1; then
  log "Installing rustup ($RUST_TOOLCHAIN)"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain "$RUST_TOOLCHAIN"
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
else
  log "Ensuring rustup toolchain ($RUST_TOOLCHAIN) is installed and default"
  rustup toolchain install "$RUST_TOOLCHAIN"
  rustup default "$RUST_TOOLCHAIN"
fi

# --- pdfium prebuilt (pinned, non-V8) ----------------------------------------
# The version marker lets a re-run notice a bumped PDFIUM_RELEASE and refetch;
# an unchanged version with the library already present is a no-op. Everything
# under vendor/pdfium/ except its README is gitignored, so this never dirties
# the tree.
if [ -f "$PDFIUM_LIB" ] && [ "$(cat "$PDFIUM_MARKER" 2>/dev/null)" = "$PDFIUM_RELEASE" ]; then
  log "pdfium $PDFIUM_RELEASE already present, skipping download"
else
  log "Downloading pdfium $PDFIUM_RELEASE (linux-x64, non-V8)"
  mkdir -p "$PDFIUM_DIR"
  curl -sL "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/$PDFIUM_RELEASE/pdfium-linux-x64.tgz" \
    | tar -xz -C "$PDFIUM_DIR"
  test -f "$PDFIUM_LIB" || { echo "libpdfium.so missing after extraction" >&2; exit 1; }
  echo "$PDFIUM_RELEASE" > "$PDFIUM_MARKER"
fi

log "Provisioning complete. Launch the app with: bash scripts/run-linux.sh"
