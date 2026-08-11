#!/usr/bin/env bash
set -euo pipefail

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly PACKAGES_DIR="${1:?usage: verify-linux-package.sh <packages-directory>}"
readonly EVIDENCE_DIR="${PACKAGE_EVIDENCE_DIR:-$REPO_ROOT/build/linux/evidence}"
readonly PACKAGE_VERSION="${PACKAGE_VERSION:-$(awk -F'"' '/^version = / { print $2; exit }' "$REPO_ROOT/Cargo.toml")}" 
readonly DEB="$PACKAGES_DIR/vitela_${PACKAGE_VERSION}_amd64.deb"
readonly APPIMAGE="$PACKAGES_DIR/Vitela-${PACKAGE_VERSION}-x86_64.AppImage"
readonly EXPECTED_RENDERED_PIXELS_SHA256='52409ebbba65a9244f7dc0d0aba4ebc529e47caf5c955da10e2a1843e9a804de'

fail() { printf 'verify-linux-package: %s\n' "$*" >&2; exit 1; }
require_file() { [ -f "$1" ] && [ -r "$1" ] || fail "required readable file not found: $1"; }
require_executable() { [ -x "$1" ] || fail "required executable not found: $1"; }
require_entry() { [ -e "$1" ] || fail "required package entry missing: $1"; }

[ "$(uname -s)" = Linux ] || fail 'verification must run on Linux'
[ "$(uname -m)" = x86_64 ] || fail 'verification requires Linux x86_64'
command -v dpkg-deb >/dev/null || fail 'dpkg-deb is required'
command -v unshare >/dev/null || fail 'unshare is required for network isolation'
require_file "$DEB"
require_file "$APPIMAGE"
require_executable "$APPIMAGE"

rm -rf -- "$EVIDENCE_DIR"
mkdir -p "$EVIDENCE_DIR/deb-root" "$EVIDENCE_DIR/appimage-root"
dpkg-deb -f "$DEB" Architecture | grep -Fx amd64 > "$EVIDENCE_DIR/deb-architecture.txt" || fail 'deb architecture is not amd64'
dpkg-deb -c "$DEB" > "$EVIDENCE_DIR/deb-listing.txt"
dpkg-deb -x "$DEB" "$EVIDENCE_DIR/deb-root"

deb_usr="$EVIDENCE_DIR/deb-root/usr"
for path in \
    "$deb_usr/bin/vitela" \
    "$deb_usr/lib/vitela/linux-gtk" \
    "$deb_usr/lib/vitela/libpdfium.so" \
    "$deb_usr/share/applications/org.vitela.Pdf.desktop" \
    "$deb_usr/share/icons/hicolor/scalable/apps/org.vitela.Pdf.svg" \
    "$deb_usr/share/doc/vitela/LICENSE-MIT" \
    "$deb_usr/share/doc/vitela/LICENSE-APACHE" \
    "$deb_usr/share/doc/vitela/pdfium/LICENSE"; do
    require_entry "$path"
done
require_executable "$deb_usr/bin/vitela"
require_executable "$deb_usr/lib/vitela/linux-gtk"
find "$deb_usr/share/doc/vitela/pdfium/licenses" -type f -print -quit | grep -q . || fail 'deb lacks PDFium third-party notices'
file -b "$deb_usr/lib/vitela/libpdfium.so" | grep -Eq 'ELF 64-bit.*x86-64' || fail 'deb PDFium is not x86_64 ELF'
readelf -d "$deb_usr/lib/vitela/linux-gtk" > "$EVIDENCE_DIR/linux-gtk-dynamic.txt"

(
    cd "$EVIDENCE_DIR/appimage-root"
    "$APPIMAGE" --appimage-extract >/dev/null
)
appdir="$EVIDENCE_DIR/appimage-root/squashfs-root"
require_executable "$appdir/AppRun"
require_entry "$appdir/usr/lib/vitela/libpdfium.so"
require_entry "$appdir/usr/share/doc/vitela/pdfium/LICENSE"
find "$appdir/usr/share/doc/vitela/pdfium/licenses" -type f -print -quit | grep -q . || fail 'AppImage lacks PDFium third-party notices'

if [ "${VERIFY_INSPECT_ONLY:-0}" = 1 ]; then
    printf 'verified Linux x86_64 package contents\n' > "$EVIDENCE_DIR/result.txt"
    exit 0
fi

env -u PDFIUM_DYNAMIC_LIB_PATH unshare --net -- "$deb_usr/bin/vitela" --package-smoke "$EVIDENCE_DIR/deb-smoke.txt"
grep -Eq '^width=[1-9][0-9]*$' "$EVIDENCE_DIR/deb-smoke.txt" || fail 'deb smoke receipt lacks rendered width'
grep -Fx "pixels_sha256=$EXPECTED_RENDERED_PIXELS_SHA256" "$EVIDENCE_DIR/deb-smoke.txt" >/dev/null || fail 'deb smoke receipt has an unexpected rendered-pixel hash'

env -u PDFIUM_DYNAMIC_LIB_PATH unshare --net -- "$APPIMAGE" --appimage-extract-and-run --package-smoke "$EVIDENCE_DIR/appimage-smoke.txt"
grep -Eq '^width=[1-9][0-9]*$' "$EVIDENCE_DIR/appimage-smoke.txt" || fail 'AppImage smoke receipt lacks rendered width'
grep -Fx "pixels_sha256=$EXPECTED_RENDERED_PIXELS_SHA256" "$EVIDENCE_DIR/appimage-smoke.txt" >/dev/null || fail 'AppImage smoke receipt has an unexpected rendered-pixel hash'

sha256sum "$DEB" "$APPIMAGE" > "$EVIDENCE_DIR/artifact-sha256.txt"
printf 'verified Linux x86_64 packages\n' > "$EVIDENCE_DIR/result.txt"
