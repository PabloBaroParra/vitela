#!/usr/bin/env bash
# Fetches and verifies the pinned Android PDFium inputs for a distributable
# build (Google Play), the Android counterpart of scripts/windows-pdfium.ps1:
#
#   scripts/fetch-android-pdfium.sh [work-dir]      (default: build/android)
#
# For each packaged ABI it downloads the pinned bblanchon/pdfium-binaries
# archive unless already present under <work-dir>/tools, refuses anything whose
# SHA-256 is not the pin, extracts it, and checks the facts this project
# requires of PDFium: release 7763 (the pdfium_7763 feature), Android, the
# right CPU, and neither V8 nor XFA. The 16-KB ELF alignment Play requires is
# checked afterwards by scripts/package-android.sh, on the copied library.
#
# It then stages the license notices the app ships as assets (PDFium's own and
# this project's) under apps/android/app/build/generated/licenses.
#
# Prints the variables package-android.sh reads, as KEY=VALUE lines, and
# appends them to $GITHUB_ENV when that is set.
set -euo pipefail

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly WORK_DIR="${1:-$REPO_ROOT/build/android}"
readonly RELEASE_URL='https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/7763'
readonly PDFIUM_BUILD='7763'
readonly LICENSE_ROOT="$REPO_ROOT/apps/android/app/build/generated/licenses/licenses"

# abi | archive | sha256 | target_cpu | package-android.sh variable
readonly INPUTS=(
    'arm64-v8a|pdfium-android-arm64.tgz|7e6a25f1a64171c174e748065a10f95edc844ee85f09fe566d5e1c50cfae14a9|arm64|PDFIUM_ANDROID_ARM64_V8A'
    'x86_64|pdfium-android-x64.tgz|bf28e8ef7dd821dc4b5818c6f91f7150f481a026990d68f64fe9797a17b2250a|x64|PDFIUM_ANDROID_X86_64'
)

fail() { printf 'fetch-android-pdfium: %s\n' "$*" >&2; exit 1; }

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{ print $1 }'
    else shasum -a 256 "$1" | awk '{ print $1 }'; fi
}

require_build_argument() {
    local args_gn="$1" pattern="$2" message="$3"
    grep -Eq "$pattern" "$args_gn" || fail "$message"
}

mkdir -p "$WORK_DIR/tools"
# Notices are assembled aside and moved into the app tree only once every
# archive passed: a refused fetch must not leave the bundle a half-cleared
# licenses/ directory to package.
licenses_staging="$WORK_DIR/licenses"
rm -rf -- "$licenses_staging"
mkdir -p "$licenses_staging/pdfium"
cp "$REPO_ROOT/LICENSE-MIT" "$REPO_ROOT/LICENSE-APACHE" "$licenses_staging/"

exports=()
for input in "${INPUTS[@]}"; do
    IFS='|' read -r abi archive expected_sha cpu variable <<< "$input"
    archive_path="$WORK_DIR/tools/$archive"
    if [ ! -f "$archive_path" ]; then
        curl -fsSL "$RELEASE_URL/$archive" -o "$archive_path.part" || fail "could not download $archive"
        mv -- "$archive_path.part" "$archive_path"
    fi
    actual_sha="$(sha256_of "$archive_path")"
    [ "$actual_sha" = "$expected_sha" ] || fail "$archive checksum mismatch: $actual_sha"

    extract_dir="$WORK_DIR/pdfium/$abi"
    rm -rf -- "$extract_dir"
    mkdir -p "$extract_dir"
    tar -xzf "$archive_path" -C "$extract_dir" || fail "$archive is unreadable"

    library="$extract_dir/lib/libpdfium.so"
    [ -f "$library" ] || fail "$archive has no lib/libpdfium.so"
    [ -f "$extract_dir/LICENSE" ] || fail "$archive has no LICENSE"
    grep -qx "BUILD=$PDFIUM_BUILD" "$extract_dir/VERSION" 2>/dev/null \
        || fail "$archive is not PDFium build $PDFIUM_BUILD"
    args_gn="$extract_dir/args.gn"
    [ -f "$args_gn" ] || fail "$archive has no args.gn"
    require_build_argument "$args_gn" '^\s*target_os\s*=\s*"android"\s*$' "$archive is not an Android build"
    require_build_argument "$args_gn" "^\\s*target_cpu\\s*=\\s*\"$cpu\"\\s*$" "$archive is not a $cpu build"
    require_build_argument "$args_gn" '^\s*pdf_enable_v8\s*=\s*false\s*$' "$archive enables V8"
    require_build_argument "$args_gn" '^\s*pdf_enable_xfa\s*=\s*false\s*$' "$archive enables XFA"

    # Both ABIs come from the same release, so one copy of the notices covers
    # them; the second copy is identical and simply overwrites the first.
    cp "$extract_dir/LICENSE" "$licenses_staging/pdfium/LICENSE"
    if [ -d "$extract_dir/licenses" ]; then cp -R "$extract_dir/licenses/." "$licenses_staging/pdfium/"; fi

    exports+=("$variable=$library")
done

rm -rf -- "$LICENSE_ROOT"
mkdir -p "$(dirname "$LICENSE_ROOT")"
mv -- "$licenses_staging" "$LICENSE_ROOT"

for line in "${exports[@]}"; do
    printf '%s\n' "$line"
    if [ -n "${GITHUB_ENV:-}" ]; then printf '%s\n' "$line" >> "$GITHUB_ENV"; fi
done
