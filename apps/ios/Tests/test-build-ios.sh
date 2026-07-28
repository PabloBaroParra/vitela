#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
build_script="$repo_root/scripts/build-ios.sh"
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

write_project() {
  printf '\t\t\t\tIPHONEOS_DEPLOYMENT_TARGET = %s;\n' "$1" > "$fixture_root/project.pbxproj"
}

write_ci() {
  printf 'IOS_DEPLOYMENT_FLOOR: "%s"\n' "$1" > "$fixture_root/ios.yml"
}

assert_rejected() {
  local description=$1
  local expected=$2
  shift 2

  if output=$(IOS_PROJECT_FILE="$fixture_root/project.pbxproj" IOS_CI_FILE="$fixture_root/ios.yml" "$@" bash "$build_script" --validate-deployment-floor 2>&1); then
    fail "deployment-floor gate accepted $description"
  fi
  assert_contains "$expected" "$output"
}

write_project '15.0'
write_ci '15.0'
IOS_DEPLOYMENT_FLOOR='15.0' bash "$build_script" --validate-deployment-floor

assert_rejected 'an absent IOS_DEPLOYMENT_FLOOR' 'IOS_DEPLOYMENT_FLOOR is required' env -u IOS_DEPLOYMENT_FLOOR
assert_rejected 'a placeholder IOS_DEPLOYMENT_FLOOR' 'IOS_DEPLOYMENT_FLOOR must be exactly 15.0' env IOS_DEPLOYMENT_FLOOR='${IOS_DEPLOYMENT_FLOOR}'
assert_rejected 'a higher IOS_DEPLOYMENT_FLOOR' 'IOS_DEPLOYMENT_FLOOR must be exactly 15.0' env IOS_DEPLOYMENT_FLOOR='16.0'

write_ci '16.0'
assert_rejected 'a CI floor mismatch' 'iOS deployment floor mismatch: CI declares 16.0, expected 15.0' env IOS_DEPLOYMENT_FLOOR='15.0'

write_ci '15.0'
write_project '16.0'
assert_rejected 'a project floor mismatch' 'iOS deployment floor mismatch: project declares 16.0, expected 15.0' env IOS_DEPLOYMENT_FLOOR='15.0'

# Regression: the gate must read the REAL Xcode build setting. An earlier
# revision matched an invented `IOS_DEPLOYMENT_FLOOR` key that Xcode ignores,
# so the project side of the comparison could never reflect a real build.
printf '\t\t\t\tIOS_DEPLOYMENT_FLOOR = 15.0;\n' > "$fixture_root/project.pbxproj"
assert_rejected 'a project declaring only the non-Xcode IOS_DEPLOYMENT_FLOOR key' 'missing iOS deployment floor in Xcode project' env IOS_DEPLOYMENT_FLOOR='15.0'

# Regression: Xcode writes a target's whole `buildSettings` dictionary on one
# line, so the gate has to match per occurrence. A line-anchored pattern found
# nothing in the real project and reported it as a missing floor.
write_ci '15.0'
printf '\t\tE01 /* Debug */ = { isa = XCBuildConfiguration; buildSettings = { IPHONEOS_DEPLOYMENT_TARGET = 15.0; SDKROOT = iphoneos; }; name = Debug; };\n' > "$fixture_root/project.pbxproj"
IOS_PROJECT_FILE="$fixture_root/project.pbxproj" IOS_CI_FILE="$fixture_root/ios.yml" \
  IOS_DEPLOYMENT_FLOOR='15.0' bash "$build_script" --validate-deployment-floor

# A project where only ONE target drifted must fail, not pass on whichever
# declaration happens to be found first.
{
  printf '\t\tE01 = { buildSettings = { IPHONEOS_DEPLOYMENT_TARGET = 15.0; }; };\n'
  printf '\t\tE02 = { buildSettings = { IPHONEOS_DEPLOYMENT_TARGET = 16.0; }; };\n'
} > "$fixture_root/project.pbxproj"
assert_rejected 'a single drifted target' 'ambiguous iOS deployment floor in Xcode project' env IOS_DEPLOYMENT_FLOOR='15.0'

# Finally, the real files: the checked-in Xcode project and workflow must both
# declare 15.0, not just the fixtures.
IOS_DEPLOYMENT_FLOOR='15.0' bash "$build_script" --validate-deployment-floor

# --prepare validates its platform argument before it checks the host, so these
# two cases are the part of it that can be exercised anywhere. Everything past
# that point needs a Mac and is covered by the build step in CI instead.
assert_prepare_rejected() {
  local description=$1
  local expected=$2
  shift 2

  if output=$(bash "$build_script" --prepare "$@" 2>&1); then
    fail "--prepare accepted $description"
  fi
  assert_contains "$expected" "$output"
}

assert_prepare_rejected 'a missing platform' '--prepare requires a platform'
assert_prepare_rejected 'an unknown platform' "unsupported iOS platform: 'android'" android
# The macOS platform name is a plausible copy/paste from build-macos.sh.
assert_prepare_rejected 'the macOS platform name' "unsupported iOS platform: 'macosx'" macosx

# --verify-deployment is about the slice that actually ships. Pointing it at the
# simulator has to be rejected rather than quietly answered, because a simulator
# slice's minos is a CI-machine requirement, not a claim about which iPhones are
# supported: the pinned PDFium 7763 simulator build declares iOS 26.0.
if output=$(bash "$build_script" --verify-deployment iphonesimulator 2>&1); then
  fail '--verify-deployment accepted the simulator slice'
fi
assert_contains 'applies to the shipped device slice' "$output"

# Mach-O parsing is pure text processing, so it can be pinned down off a Mac by
# sourcing the script and feeding it recorded vtool output. Sourcing with no
# arguments only prints usage.
# shellcheck source=/dev/null
source "$build_script" >/dev/null

assert_versions() {
  local description=$1
  local expected=$2
  local fixture=$3
  local actual

  actual=$(printf '%s\n' "$fixture" | extract_ios_versions | tr '\n' ' ')
  actual=${actual% }
  [[ "$actual" == "$expected" ]] || fail "$description: expected '$expected', got '$actual'"
}

assert_versions 'LC_BUILD_VERSION minos' '15.0' 'Load command 8
      cmd LC_BUILD_VERSION
  cmdsize 32
 platform IOS
    minos 15.0
      sdk 18.0'

# Regression: rustc's default deployment target for aarch64-apple-ios is low
# enough that the linker emits the OLD load command, which has no minos field.
# Reading only minos reported "could not determine the deployment target" on a
# binary that was declaring one perfectly clearly.
assert_versions 'LC_VERSION_MIN_IPHONEOS version' '10.0' 'Load command 8
      cmd LC_VERSION_MIN_IPHONEOS
  cmdsize 16
  version 10.0
      sdk 18.0'

# LC_ID_DYLIB carries its own unrelated "version" fields; matching `version`
# without tracking the enclosing command would read them as deployment targets.
assert_versions 'LC_ID_DYLIB version fields are ignored' '15.0' 'Load command 2
          cmd LC_ID_DYLIB
      cmdsize 48
         name @rpath/libpdf_ffi.dylib
   time stamp 1
      current version 0.0.0
compatibility version 0.0.0
Load command 8
      cmd LC_BUILD_VERSION
    minos 15.0'

printf 'PASS: iOS deployment floor is exactly 15.0 in the real Xcode project, and absent, placeholder, higher, conflicting, and non-Xcode-key values fail closed\n'
