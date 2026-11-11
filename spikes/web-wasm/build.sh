#!/usr/bin/env bash
# Builds the browser half of the spike into www/ and prints bundle sizes.
# Needs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli matching
# Cargo.lock's wasm-bindgen (0.2.130), gh (to fetch pdfium), gzip.
set -euo pipefail
cd "$(dirname "$0")"

PDFIUM_TAG="chromium/7763"   # must match pdfium-render's `pdfium_7763` feature
ROOT=../..

cargo build --target wasm32-unknown-unknown --release
wasm-bindgen --target no-modules --no-typescript \
  --remove-name-section --remove-producers-section --out-dir www/pkg \
  target/wasm32-unknown-unknown/release/web_wasm_spike.wasm

if [ ! -f www/pdfium/pdfium.wasm ]; then
  tmp=$(mktemp -d)
  gh release download "$PDFIUM_TAG" -R bblanchon/pdfium-binaries -p pdfium-wasm.tgz -D "$tmp"
  tar xzf "$tmp/pdfium-wasm.tgz" -C "$tmp"
  mkdir -p www/pdfium
  cp "$tmp/lib/pdfium.js" "$tmp/lib/pdfium.wasm" "$tmp/VERSION" www/pdfium/
  rm -rf "$tmp"
fi

mkdir -p www/fixtures
cp "$ROOT/tests/fixtures/forms/reportlab_acroform.pdf" \
   "$ROOT/tests/fixtures/content-edit/reportlab_embedded_subset.pdf" \
   "$ROOT/tests/fixtures/compress/scan_200dpi.pdf" www/fixtures/

size() { printf '%-34s raw %9d  gzip -9 %9d\n' "$1" "$(wc -c <"$1")" "$(gzip -9c "$1" | wc -c)"; }
for f in www/pkg/web_wasm_spike_bg.wasm www/pkg/web_wasm_spike.js \
         www/pdfium/pdfium.wasm www/pdfium/pdfium.js; do size "$f"; done
