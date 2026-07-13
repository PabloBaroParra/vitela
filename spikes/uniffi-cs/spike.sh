#!/usr/bin/env bash
# Batch 1 spike (T-006..T-010): rebuild the Rust cdylib, regenerate the C#
# bindings, build the C# host, and run it end-to-end. Every number the host
# prints comes from a real measured run — nothing here is simulated.
#
# Prerequisites:
#   - Rust toolchain (rustc/cargo) on PATH
#   - uniffi-bindgen-cs, pinned to the tag matching the `uniffi` crate
#     version in Cargo.toml (see README.md "Version pinning" section):
#       cargo install uniffi-bindgen-cs --git https://github.com/NordSecurity/uniffi-bindgen-cs --tag v0.11.0+v0.31.0 --locked
#   - .NET SDK 8.0+ (`dotnet --version`)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

echo "== cargo test (strict TDD: Rust-side unit tests) =="
cargo test --release

echo "== cargo build --release (produces the cdylib the C# host loads) =="
cargo build --release

DLL_NAME="uniffi_cs_spike.dll"
DLL_PATH="target/release/${DLL_NAME}"
if [ ! -f "$DLL_PATH" ]; then
  echo "ERROR: expected $DLL_PATH after cargo build --release (are you on Windows? this repo currently targets Windows only)." >&2
  exit 1
fi

echo "== uniffi-bindgen-cs: regenerate C# bindings from the built cdylib =="
uniffi-bindgen-cs --library "$DLL_PATH" --out-dir csharp-host/generated --no-format

echo "== dotnet build -c Release =="
(cd csharp-host && dotnet build -c Release)

echo "== copy native library next to the managed host so DllImport can find it =="
cp "$DLL_PATH" csharp-host/bin/Release/net9.0/"${DLL_NAME}"

echo "== run =="
(cd csharp-host && dotnet bin/Release/net9.0/SpikeHost.dll)
