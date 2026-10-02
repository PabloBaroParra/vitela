#!/usr/bin/env bash
# Shell tests for scripts/fetch-android-pdfium.sh that need no network: an
# archive already present under <work-dir>/tools is used as is, so planting a
# file there exercises the checksum gate without downloading anything. The
# accepting path runs against the real pinned archives in android.yml.
set -euo pipefail

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly FETCH_SCRIPT="$REPO_ROOT/scripts/fetch-android-pdfium.sh"

fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

work=''
cleanup() { [ -z "$work" ] || rm -rf -- "$work"; }
trap cleanup EXIT

readonly LICENSE_ROOT="$REPO_ROOT/apps/android/app/build/generated/licenses/licenses"

test_tampered_archive_is_refused_before_extraction() {
    work="$(mktemp -d)"
    # A refused fetch must leave whatever a previous good fetch staged alone:
    # the bundle packages this tree, so a half-cleared one ships without the
    # PDFium notices.
    local sentinel="$LICENSE_ROOT/pdfium/test-sentinel"
    mkdir -p "$LICENSE_ROOT/pdfium"
    printf 'staged\n' > "$sentinel"
    mkdir -p "$work/tools"
    printf 'not a pdfium archive\n' > "$work/tools/pdfium-android-arm64.tgz"
    local output
    if output="$(bash "$FETCH_SCRIPT" "$work" 2>&1)"; then
        fail 'accepted an archive whose checksum is not the pin'
    fi
    grep -q 'pdfium-android-arm64.tgz checksum mismatch' <<< "$output" \
        || fail "refused for the wrong reason: $output"
    [ ! -e "$work/pdfium/arm64-v8a" ] || fail 'extracted an archive that failed its checksum'
    grep -q 'PDFIUM_ANDROID' <<< "$output" && fail 'exported a library path after a refusal'
    [ -f "$sentinel" ] || fail 'a refused fetch cleared the staged license notices'
    rm -f -- "$sentinel"
    cleanup; work=''
}

test_tampered_archive_is_refused_before_extraction
printf 'fetch-android-pdfium shell tests: 1 passed\n'
