#!/usr/bin/env bash
# Builds the two WASM modules the web worker loads into apps/web/generated/:
#   generated/render/  the temporary Rust render crate (render-wasm/), via
#                      wasm-bindgen --target web (an ES module Vite bundles)
#   generated/pdfium/  bblanchon's Emscripten pdfium, sha256-pinned
#
# Needs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli matching
# render-wasm/Cargo.toml's wasm-bindgen pin, curl, sha256sum, tar.
set -euo pipefail
cd "$(dirname "$0")/.."

# Must match pdfium-render's `pdfium_7763` feature in render-wasm/Cargo.toml.
PDFIUM_TAG="chromium/7763"
# sha256 of that release's pdfium-wasm.tgz, as GitHub reports it (`gh release
# view chromium/7763 -R bblanchon/pdfium-binaries --json assets`) and as the
# phase-0 spike pinned it (spikes/web-wasm/build.sh). A release asset can be
# replaced under the same tag; this refuses it if so.
PDFIUM_SHA256="213cad62710dd7b9a6eff84fe35a4b7161aeba8aa41e4c9b46463c09a85601ca"
PDFIUM_URL="https://github.com/bblanchon/pdfium-binaries/releases/download/${PDFIUM_TAG/\//%2F}/pdfium-wasm.tgz"

WASM_BINDGEN_VERSION="0.2.130"
if [ "$(wasm-bindgen --version 2>/dev/null)" != "wasm-bindgen $WASM_BINDGEN_VERSION" ]; then
  echo "error: need wasm-bindgen-cli $WASM_BINDGEN_VERSION" >&2
  echo "  cargo install wasm-bindgen-cli --version $WASM_BINDGEN_VERSION --locked" >&2
  exit 1
fi

cargo build --locked --release --target wasm32-unknown-unknown \
  --manifest-path render-wasm/Cargo.toml
rm -rf generated/render
wasm-bindgen --target web \
  --remove-name-section --remove-producers-section --out-dir generated/render \
  render-wasm/target/wasm32-unknown-unknown/release/vitela_web_render.wasm

# Re-fetch only when the cached copy is from another tag or hash
# (generated/pdfium/PIN records both).
pin="$PDFIUM_TAG $PDFIUM_SHA256"
if [ "$(cat generated/pdfium/PIN 2>/dev/null)" != "$pin" ]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  curl -fsSL --retry 3 -o "$tmp/pdfium-wasm.tgz" "$PDFIUM_URL"
  echo "$PDFIUM_SHA256  $tmp/pdfium-wasm.tgz" | sha256sum -c -
  tar xzf "$tmp/pdfium-wasm.tgz" -C "$tmp"
  rm -rf generated/pdfium && mkdir -p generated/pdfium
  cp "$tmp/lib/pdfium.js" "$tmp/lib/pdfium.wasm" "$tmp/VERSION" generated/pdfium/
  echo "$pin" > generated/pdfium/PIN
fi
