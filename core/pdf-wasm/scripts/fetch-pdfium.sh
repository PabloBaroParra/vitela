#!/usr/bin/env bash
# Fetches bblanchon's Emscripten pdfium (pdfium.js + pdfium.wasm), sha256-
# pinned, into the directory given as $1. One pin for everything that loads
# pdfium in a browser: apps/web's build (scripts/build-wasm.sh) and this
# crate's wasm-bindgen tests (PDFIUM_WASM_DIR, see tests/engine.rs).
#
# Re-fetches only when the cached copy is from another tag or hash ($1/PIN
# records both). Needs: curl, sha256sum, tar.
set -euo pipefail

dest="${1:?usage: fetch-pdfium.sh <dest-dir>}"

# Must match pdfium-render's `pdfium_7763` feature (core/pdf-render/Cargo.toml).
PDFIUM_TAG="chromium/7763"
# sha256 of that release's pdfium-wasm.tgz, as GitHub reports it (`gh release
# view chromium/7763 -R bblanchon/pdfium-binaries --json assets`) and as the
# phase-0 spike pinned it (spikes/web-wasm/build.sh). A release asset can be
# replaced under the same tag; this refuses it if so.
PDFIUM_SHA256="213cad62710dd7b9a6eff84fe35a4b7161aeba8aa41e4c9b46463c09a85601ca"
PDFIUM_URL="https://github.com/bblanchon/pdfium-binaries/releases/download/${PDFIUM_TAG/\//%2F}/pdfium-wasm.tgz"

pin="$PDFIUM_TAG $PDFIUM_SHA256"
if [ "$(cat "$dest/PIN" 2>/dev/null)" = "$pin" ]; then
  exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL --retry 3 -o "$tmp/pdfium-wasm.tgz" "$PDFIUM_URL"
echo "$PDFIUM_SHA256  $tmp/pdfium-wasm.tgz" | sha256sum -c -
tar xzf "$tmp/pdfium-wasm.tgz" -C "$tmp"
rm -rf "$dest" && mkdir -p "$dest"
cp "$tmp/lib/pdfium.js" "$tmp/lib/pdfium.wasm" "$tmp/VERSION" "$dest/"
echo "$pin" > "$dest/PIN"
