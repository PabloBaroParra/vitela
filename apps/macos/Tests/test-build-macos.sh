#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
# Always invoked through `bash`: every script in this repo is committed
# mode 100644, so exec-ing it directly is a "Permission denied" on macOS.
build_script="$repo_root/scripts/build-macos.sh"
project_file="$repo_root/apps/macos/Vitela.xcodeproj/project.pbxproj"
fixture_root=$(mktemp -d)
trap 'rm -rf "$fixture_root"' EXIT

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

assert_contains() {
  local expected=$1
  local actual=$2
  [[ "$actual" == *"$expected"* ]] || fail "expected '$expected' in '$actual'"
}

write_vtool() {
  mkdir -p "$fixture_root/bin"
  cat > "$fixture_root/bin/vtool" <<'EOF'
#!/usr/bin/env bash
if [[ "$1" == "-show-build" ]]; then
  cat "$2.minos"
  exit 0
fi
exit 64
EOF
  chmod +x "$fixture_root/bin/vtool"
}

write_macho_fixture() {
  local name=$1
  local minos=$2
  : > "$fixture_root/$name"
  printf 'minos %s\n' "$minos" > "$fixture_root/$name.minos"
}

write_vtool
# A binary asking for an OLDER macOS than the floor is fine — the gate exists
# to catch ones that demand a newer macOS than Vitela supports.
write_macho_fixture compatible.dylib 11.0
write_macho_fixture at_floor.dylib 12.0

PATH="$fixture_root/bin:$PATH" bash "$build_script" --check-deployment \
  "$fixture_root/compatible.dylib" "$fixture_root/at_floor.dylib"

write_macho_fixture incompatible.dylib 13.0
if output=$(PATH="$fixture_root/bin:$PATH" bash "$build_script" --check-deployment "$fixture_root/incompatible.dylib" 2>&1); then
  fail "deployment gate accepted an incompatible Mach-O fixture"
fi
assert_contains "requires macOS 13.0" "$output"

write_macho_fixture universal.dylib $'12.0\nminos 13.0'
if output=$(PATH="$fixture_root/bin:$PATH" bash "$build_script" --check-deployment "$fixture_root/universal.dylib" 2>&1); then
  fail "deployment gate accepted an incompatible slice in a universal Mach-O fixture"
fi
assert_contains "requires macOS 13.0" "$output"

project=$(cat "$project_file")
assert_contains "MACOSX_DEPLOYMENT_TARGET = 12.0;" "$project"
assert_contains "VitelaTests" "$project"

# The app target once compiled the generated bindings without linking or
# including anything they need, so it could never link. These four settings are
# what make `pdf_ffi.swift` resolve, and a bundle that launches at all.
assert_contains "OTHER_LDFLAGS = \"-lpdf_ffi\";" "$project"
assert_contains "SWIFT_INCLUDE_PATHS = \"\$(SRCROOT)/Generated\";" "$project"
assert_contains "LD_RUNPATH_SEARCH_PATHS = \"@executable_path/../Frameworks\";" "$project"
assert_contains "GENERATE_INFOPLIST_FILE = YES;" "$project"

# cargo builds libpdf_ffi.dylib for the host triple only. Letting Xcode go
# universal makes the x86_64 slice fail to link against an arm64-only dylib, so
# the app arch must stay pinned to the host until the Rust side is lipo'd
# (T-059).
assert_contains "ARCHS = \"\$(NATIVE_ARCH_ACTUAL)\";" "$project"

# A test bundle that cannot be loaded into its host is a test that never runs.
assert_contains "com.apple.product-type.bundle.unit-test" "$project"
[[ "$project" != *"bundle.ui-testing"* ]] \
  || fail "view-model tests must live in a unit-test target, not a UI-testing one"

# CI drives `xcodebuild -scheme Vitela`; without a shared scheme that depends on
# whatever Xcode happens to autocreate on the runner.
scheme="$repo_root/apps/macos/Vitela.xcodeproj/xcshareddata/xcschemes/Vitela.xcscheme"
[[ -f "$scheme" ]] || fail "missing shared scheme: $scheme"
assert_contains "VitelaTests.xctest" "$(cat "$scheme")"

printf 'PASS: deployment gate accepts macOS 12.0 and older, rejects macOS 13.0, and the project links, bundles, and tests what it declares\n'
