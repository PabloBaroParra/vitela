#!/usr/bin/env bash
# Portable checks of the distribution packaging: no signing identity is
# needed, so this runs on every CI machine. What it pins is the part a
# mistake would only reveal at Apple's end — the entitlements each channel
# signs with, and that a missing credential stops the script before it signs.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
package_script="$repo_root/scripts/package-macos.sh"
distribution="$repo_root/apps/macos/Distribution"

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

plist_value() {
  /usr/libexec/PlistBuddy -c "Print :$2" "$1" 2>/dev/null || true
}

assert_refuses_without() {
  local variable=$1
  shift
  local output
  if output=$(env -u VITELA_DEVELOPER_ID -u VITELA_NOTARY_PROFILE -u VITELA_APP_STORE_ID \
      -u VITELA_INSTALLER_ID -u VITELA_PROFILE -u APPLE_API_KEY_PATH -u APPLE_API_KEY_ID \
      -u APPLE_API_ISSUER bash "$package_script" "$@" 2>&1); then
    fail "'$*' ran without $variable"
  fi
  [[ "$output" == *"$variable is not set"* ]] || fail "expected '$variable is not set' from '$*', got: $output"
}

bash -n "$package_script"
bash "$package_script" --help | grep -q 'developer-id' || fail "--help does not describe developer-id"

assert_refuses_without VITELA_DEVELOPER_ID developer-id
assert_refuses_without VITELA_APP_STORE_ID app-store
fixture=$(mktemp)
trap 'rm -f "$fixture"' EXIT
assert_refuses_without APPLE_API_KEY_PATH upload "$fixture"

if [[ "$(uname -s)" == "Darwin" ]]; then
  plutil -lint "$distribution/AppStore.entitlements" "$distribution/DeveloperID.entitlements" \
    "$repo_root/apps/macos/Vitela/PrivacyInfo.xcprivacy" >/dev/null
  store="$distribution/AppStore.entitlements"
  developer="$distribution/DeveloperID.entitlements"
  [[ "$(plist_value "$store" com.apple.security.app-sandbox)" == "true" ]] \
    || fail "the App Store build must be sandboxed"
  [[ "$(plist_value "$store" com.apple.security.files.bookmarks.app-scope)" == "true" ]] \
    || fail "Recent needs app-scoped bookmarks in the sandbox"
  [[ -z "$(plist_value "$store" com.apple.security.network.client)" ]] \
    || fail "Vitela works offline; the App Store build must not ask for the network"
  [[ -z "$(plist_value "$developer" com.apple.security.app-sandbox)" ]] \
    || fail "the Developer ID build keeps card or token signing, which the sandbox refuses"
  [[ "$(plist_value "$developer" com.apple.security.cs.disable-library-validation)" == "true" ]] \
    || fail "the hardened runtime needs library validation off to load a PKCS#11 module"
fi

printf 'PASS: packaging refuses missing credentials and signs each channel with its own entitlements\n'
