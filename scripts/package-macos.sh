#!/usr/bin/env bash
# Signs an assembled Vitela.app (scripts/build-macos.sh --assemble) for one of
# the two ways a Mac gets it, and packages it:
#
#   developer-id  Developer ID signature with the hardened runtime, notarized
#                 by Apple and stapled, zipped for GitHub Releases. Runs
#                 outside the sandbox (apps/macos/Distribution/DeveloperID.entitlements).
#   app-store     Apple Distribution signature inside the App Sandbox
#                 (apps/macos/Distribution/AppStore.entitlements) with the
#                 provisioning profile embedded, as an installer package for
#                 App Store Connect.
#   upload        Validates and uploads that package to App Store Connect.
#
# The credentials are the maintainer's and never live in the repository; how
# to obtain each one is docs/macos-distribution.md. Every step is fail-closed:
# a signature, notarization or validation that does not pass stops the script.
set -euo pipefail

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage:
  scripts/package-macos.sh developer-id [Vitela.app]
  scripts/package-macos.sh app-store [Vitela.app]
  scripts/package-macos.sh upload <Vitela.pkg>

The app defaults to build/Vitela.app. Output goes to build/.

developer-id needs:
  VITELA_DEVELOPER_ID      "Developer ID Application: <Name> (<TEAMID>)"
  and either VITELA_NOTARY_PROFILE (a `notarytool store-credentials` profile)
  or the App Store Connect API key below.

app-store needs:
  VITELA_APP_STORE_ID      "Apple Distribution: <Name> (<TEAMID>)"
  VITELA_INSTALLER_ID      "3rd Party Mac Developer Installer: <Name> (<TEAMID>)"
  VITELA_PROFILE           the Mac App Store .provisionprofile for the bundle id

upload (and notarization without a profile) needs the App Store Connect API key:
  APPLE_API_KEY_PATH       AuthKey_<KEYID>.p8
  APPLE_API_KEY_ID         its key ID
  APPLE_API_ISSUER         the issuer ID shown above the keys list
EOF
}

require() {
  local name
  for name in "$@"; do
    [[ -n "${!name:-}" ]] || die "$name is not set (see scripts/package-macos.sh --help)"
  done
}

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
distribution="$repo_root/apps/macos/Distribution"
out="$repo_root/build"

# A fresh copy, so the ad-hoc development bundle stays as it was.
stage() {
  local source=$1 target=$2
  [[ "$(uname -s)" == "Darwin" ]] || die "signing requires macOS"
  bash "$repo_root/scripts/build-macos.sh" --validate-bundle-layout "$source"
  rm -rf "$target"
  mkdir -p "$(dirname "$target")"
  ditto "$source" "$target"
}

info() {
  /usr/libexec/PlistBuddy -c "Print :$2" "$1/Contents/Info.plist"
}

# Nested code first, the app last: signing the app seals what is inside it.
sign_bundle() {
  local app=$1 identity=$2 entitlements=$3
  shift 3
  local library
  for library in "$app"/Contents/Frameworks/*.dylib; do
    codesign --force --timestamp "$@" --sign "$identity" "$library"
  done
  codesign --force --timestamp "$@" --entitlements "$entitlements" --sign "$identity" "$app"
  codesign --verify --deep --strict --verbose=2 "$app"
}

notary_auth() {
  if [[ -n "${VITELA_NOTARY_PROFILE:-}" ]]; then
    printf '%s\n' --keychain-profile "$VITELA_NOTARY_PROFILE"
  else
    require APPLE_API_KEY_PATH APPLE_API_KEY_ID APPLE_API_ISSUER
    printf '%s\n' --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER"
  fi
}

developer_id() {
  require VITELA_DEVELOPER_ID
  local source=${1:-$out/Vitela.app}
  local app="$out/developer-id/Vitela.app"
  stage "$source" "$app"
  sign_bundle "$app" "$VITELA_DEVELOPER_ID" "$distribution/DeveloperID.entitlements" --options runtime

  local version zip auth=()
  version=$(info "$app" CFBundleShortVersionString)
  zip="$out/Vitela-$version-macos.zip"
  while IFS= read -r line; do auth+=("$line"); done < <(notary_auth)
  ditto -c -k --keepParent "$app" "$out/developer-id/notarize.zip"
  # --wait returns non-zero only when the request fails; the verdict is in
  # the status line, so "Invalid" has to be caught here.
  local result
  result=$(xcrun notarytool submit "$out/developer-id/notarize.zip" "${auth[@]}" --wait)
  printf '%s\n' "$result"
  grep -q 'status: Accepted' <<< "$result" || die "notarization was not accepted (xcrun notarytool log <id> says why)"
  xcrun stapler staple "$app"
  xcrun stapler validate "$app"
  spctl --assess --type execute --verbose=2 "$app"
  rm -f "$zip" "$out/developer-id/notarize.zip"
  ditto -c -k --keepParent "$app" "$zip"
  printf 'ok: %s\n' "$zip"
}

# The App Store signature carries the application and team identifiers the
# profile grants; the rest of the entitlements are the sandbox's.
app_store_entitlements() {
  local profile=$1 target=$2
  local decoded
  decoded=$(mktemp)
  security cms -D -i "$profile" > "$decoded" || die "cannot read the provisioning profile $profile"
  cp "$distribution/AppStore.entitlements" "$target"
  local key value
  for key in com.apple.application-identifier com.apple.developer.team-identifier; do
    value=$(/usr/libexec/PlistBuddy -c "Print :Entitlements:$key" "$decoded") \
      || die "the provisioning profile grants no $key"
    /usr/libexec/PlistBuddy -c "Add :$key string $value" "$target"
  done
  rm -f "$decoded"
}

app_store() {
  require VITELA_APP_STORE_ID VITELA_INSTALLER_ID VITELA_PROFILE
  [[ -f "$VITELA_PROFILE" ]] || die "VITELA_PROFILE does not name a file: $VITELA_PROFILE"
  local source=${1:-$out/Vitela.app}
  local app="$out/app-store/Vitela.app"
  stage "$source" "$app"

  local bundle_id application_id
  bundle_id=$(info "$app" CFBundleIdentifier)
  cp "$VITELA_PROFILE" "$app/Contents/embedded.provisionprofile"
  app_store_entitlements "$VITELA_PROFILE" "$out/app-store/Vitela.entitlements"
  application_id=$(/usr/libexec/PlistBuddy -c "Print :com.apple.application-identifier" "$out/app-store/Vitela.entitlements")
  [[ "$application_id" == *".$bundle_id" ]] \
    || die "the profile is for $application_id, but the app is $bundle_id (set VITELA_BUNDLE_ID when assembling)"
  sign_bundle "$app" "$VITELA_APP_STORE_ID" "$out/app-store/Vitela.entitlements"

  local version build pkg
  version=$(info "$app" CFBundleShortVersionString)
  build=$(info "$app" CFBundleVersion)
  pkg="$out/Vitela-$version-$build-app-store.pkg"
  productbuild --component "$app" /Applications --sign "$VITELA_INSTALLER_ID" "$pkg"
  pkgutil --check-signature "$pkg"
  printf 'ok: %s\n' "$pkg"
}

upload() {
  local pkg=${1:-}
  [[ -f "$pkg" ]] || die "upload requires the .pkg app-store produced"
  require APPLE_API_KEY_PATH APPLE_API_KEY_ID APPLE_API_ISSUER
  # altool reads the key from a private_keys directory named after its ID.
  local keys
  keys=$(mktemp -d)
  trap 'rm -rf "$keys"' RETURN
  cp "$APPLE_API_KEY_PATH" "$keys/AuthKey_$APPLE_API_KEY_ID.p8"
  local auth=(--apiKey "$APPLE_API_KEY_ID" --apiIssuer "$APPLE_API_ISSUER")
  API_PRIVATE_KEYS_DIR="$keys" xcrun altool --validate-app -f "$pkg" -t macos "${auth[@]}"
  API_PRIVATE_KEYS_DIR="$keys" xcrun altool --upload-app -f "$pkg" -t macos "${auth[@]}"
}

case ${1:-} in
  developer-id) shift; developer_id "$@" ;;
  app-store) shift; app_store "$@" ;;
  upload) shift; upload "$@" ;;
  --help|-h|"") usage ;;
  *) die "unknown command: $1" ;;
esac
