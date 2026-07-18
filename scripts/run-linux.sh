#!/usr/bin/env bash
#
# Build and run the Linux GTK4 shell (apps/linux-gtk) with pdfium wired up,
# under WSL2 + WSLg. Run scripts/linux-dev-setup.sh at least once first.
#
#   bash scripts/run-linux.sh              # open the app
#   bash scripts/run-linux.sh --release    # extra args pass through to cargo
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PDFIUM_LIB="$REPO_ROOT/core/pdf-render/vendor/pdfium/lib/libpdfium.so"

if [ ! -f "$PDFIUM_LIB" ]; then
  echo "pdfium not found at $PDFIUM_LIB" >&2
  echo "Run: bash scripts/linux-dev-setup.sh" >&2
  exit 1
fi

# Make cargo reachable even from a non-login shell that never sourced the
# rustup env.
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

# On Linux the vendored pdfium lands in lib/ (not bin/), so the zero-config
# vendored-dir lookup misses it — hand the path over explicitly, same as CI.
export PDFIUM_DYNAMIC_LIB_PATH="$PDFIUM_LIB"

exec cargo run -p linux-gtk "$@"
