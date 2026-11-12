#!/usr/bin/env bash
# Builds the two WASM modules the web worker loads into apps/web/generated/:
#   generated/engine/  core/pdf-wasm, via wasm-bindgen --target web (an ES
#                      module Vite bundles)
#   generated/pdfium/  bblanchon's Emscripten pdfium, sha256-pinned by
#                      core/pdf-wasm/scripts/fetch-pdfium.sh
#
# Needs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli matching
# core/pdf-wasm/Cargo.toml's wasm-bindgen pin, curl, sha256sum, tar.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=../..

WASM_BINDGEN_VERSION="0.2.130"
if [ "$(wasm-bindgen --version 2>/dev/null)" != "wasm-bindgen $WASM_BINDGEN_VERSION" ]; then
  echo "error: need wasm-bindgen-cli $WASM_BINDGEN_VERSION" >&2
  echo "  cargo install wasm-bindgen-cli --version $WASM_BINDGEN_VERSION --locked" >&2
  exit 1
fi

# The root workspace's `wasm-release` profile: release + LTO + 1 CGU.
cargo build --locked --profile wasm-release --target wasm32-unknown-unknown \
  --manifest-path "$ROOT/Cargo.toml" -p pdf-wasm
rm -rf generated/engine
wasm-bindgen --target web \
  --remove-name-section --remove-producers-section --out-dir generated/engine \
  "$ROOT/target/wasm32-unknown-unknown/wasm-release/pdf_wasm.wasm"

"$ROOT/core/pdf-wasm/scripts/fetch-pdfium.sh" generated/pdfium
