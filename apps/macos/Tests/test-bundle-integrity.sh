#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
# Always invoked through `bash`: every script in this repo is committed
# mode 100644, so exec-ing it directly is a "Permission denied" on macOS.
build_script="$repo_root/scripts/build-macos.sh"
fixture_root=$(mktemp -d)
trap 'rm -rf "$fixture_root"' EXIT

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

app="$fixture_root/Vitela.app"

write_complete_bundle() {
  rm -rf "$app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Frameworks"
  touch "$app/Contents/MacOS/Vitela"
  chmod +x "$app/Contents/MacOS/Vitela"
  touch "$app/Contents/Info.plist"
  touch "$app/Contents/Frameworks/libpdf_ffi.dylib"
  touch "$app/Contents/Frameworks/libpdfium.dylib"
}

# Each case removes exactly one required member from an otherwise complete
# bundle, so a passing run means the gate rejects that member's absence
# specifically — not that it rejects everything.
assert_rejects_missing() {
  local removed=$1
  local expected=$2

  write_complete_bundle
  rm "$app/$removed"
  if output=$(bash "$build_script" --validate-bundle-layout "$app" 2>&1); then
    fail "bundle validation accepted a bundle missing $removed"
  fi
  [[ "$output" == *"$expected"* ]] || fail "expected '$expected' for missing $removed, got: $output"
}

write_complete_bundle
bash "$build_script" --validate-bundle-layout "$app"

assert_rejects_missing "Contents/Frameworks/libpdfium.dylib" "missing bundled libpdfium.dylib"
assert_rejects_missing "Contents/Frameworks/libpdf_ffi.dylib" "missing bundled libpdf_ffi.dylib"
assert_rejects_missing "Contents/MacOS/Vitela" "missing application executable"
# A bundle without Info.plist passes every Mach-O check and still cannot launch.
assert_rejects_missing "Contents/Info.plist" "missing Contents/Info.plist"

printf 'PASS: bundle layout requires the app executable, Info.plist, FFI, and PDFium dylibs\n'
