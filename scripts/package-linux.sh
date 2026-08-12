#!/usr/bin/env bash
set -euo pipefail

readonly PDFIUM_VERSION='148.0.7763.0'
readonly PDFIUM_SHA256_DEFAULT='e3f0c66b2daad710cb6c8edd4a8c45c8902995e359dc0775917fc16e2e56349d'
readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly BUILD_ROOT="${PACKAGE_BUILD_ROOT:-$REPO_ROOT/build/linux}"
readonly PDFIUM_ARCHIVE="${PDFIUM_ARCHIVE:?PDFIUM_ARCHIVE must name the verified pdfium-linux-x64.tgz input}"
readonly PDFIUM_SHA256="${PDFIUM_SHA256:-$PDFIUM_SHA256_DEFAULT}"
readonly LINUX_GTK_BINARY="${LINUX_GTK_BINARY:-$REPO_ROOT/target/release/linux-gtk}"
readonly PACKAGE_VERSION="${PACKAGE_VERSION:-$(awk -F'"' '/^version = / { print $2; exit }' "$REPO_ROOT/Cargo.toml")}" 

fail() { printf 'package-linux: %s\n' "$*" >&2; exit 1; }
require_tool() { command -v "$1" >/dev/null 2>&1 || fail "required tool not found: $1"; }
require_file() { [ -f "$1" ] && [ -r "$1" ] || fail "required readable file not found: $1"; }

require_tool tar
require_tool sha256sum
require_tool file
require_tool readelf
require_tool dpkg-deb
require_file "$PDFIUM_ARCHIVE"
require_file "$LINUX_GTK_BINARY"
[ "$(sha256sum "$PDFIUM_ARCHIVE" | awk '{print $1}')" = "$PDFIUM_SHA256" ] || fail 'PDFium archive checksum mismatch'

work_dir="$BUILD_ROOT/work"
stage_usr="$BUILD_ROOT/stage/AppDir/usr"
packages_dir="$BUILD_ROOT/packages"
deb_root="$work_dir/deb-root"
rm -rf -- "$work_dir" "$stage_usr" "$packages_dir"
mkdir -p "$work_dir/pdfium" "$stage_usr" "$deb_root/DEBIAN" "$deb_root/usr" "$work_dir/packages"
staged_packages="$work_dir/packages"

require_tool "${LINUXDEPLOY:-linuxdeploy}"
require_tool "${APPIMAGETOOL:-appimagetool}"

archive_contents="$work_dir/pdfium-contents"
tar -tzf "$PDFIUM_ARCHIVE" > "$archive_contents" || fail 'PDFium archive is unreadable'
grep -Eq '(^|/)lib/libpdfium\.so$' "$archive_contents" || fail 'PDFium archive lacks lib/libpdfium.so'
tar -xzf "$PDFIUM_ARCHIVE" --no-same-owner --no-same-permissions -C "$work_dir/pdfium"
[ -z "$(find "$work_dir/pdfium" -type l -print -quit)" ] || fail 'PDFium archive contains a symlink'
require_file "$work_dir/pdfium/VERSION"
require_file "$work_dir/pdfium/args.gn"
require_file "$work_dir/pdfium/LICENSE"
require_file "$work_dir/pdfium/lib/libpdfium.so"
pdfium_archive_version="$(awk '
    BEGIN { valid = 1 }
    { sub(/\r$/, "") }
    !/^(MAJOR|MINOR|BUILD|PATCH)=[0-9]+$/ { valid = 0; exit }
    {
        split($0, fields, "=")
        if (seen[fields[1]]++) {
            valid = 0
            exit
        }
        value[fields[1]] = fields[2]
    }
    END {
        if (!valid || NR != 4 || !("MAJOR" in value) || !("MINOR" in value) || !("BUILD" in value) || !("PATCH" in value)) {
            exit 1
        }
        printf "%s.%s.%s.%s\n", value["MAJOR"], value["MINOR"], value["BUILD"], value["PATCH"]
    }
' "$work_dir/pdfium/VERSION")" || fail 'PDFium VERSION metadata is malformed'
[ "$pdfium_archive_version" = "$PDFIUM_VERSION" ] || fail 'PDFium version is not 148.0.7763.0'
grep -Eq 'target_os[[:space:]]*=[[:space:]]*"linux"' "$work_dir/pdfium/args.gn" || fail 'PDFium input is not Linux'
grep -Eq 'target_cpu[[:space:]]*=[[:space:]]*"x64"' "$work_dir/pdfium/args.gn" || fail 'PDFium input is not x64'
awk '
    { sub(/\r$/, "") }
    !/^[[:space:]]*#/ && /^[[:space:]]*pdf_enable_v8([[:space:]]|=)/ { assignments++ }
    !/^[[:space:]]*#/ && /^[[:space:]]*pdf_enable_v8[[:space:]]*=[[:space:]]*false[[:space:]]*$/ { disabled++ }
    END { exit !(assignments == 1 && disabled == 1) }
' "$work_dir/pdfium/args.gn" || fail 'PDFium input enables V8'
grep -Eq 'pdf_enable_xfa[[:space:]]*=[[:space:]]*false' "$work_dir/pdfium/args.gn" || fail 'PDFium input enables XFA'
file -b "$work_dir/pdfium/lib/libpdfium.so" | grep -Eq 'ELF 64-bit.*x86-64' || fail 'PDFium library is not an x86_64 ELF'
readelf -h "$work_dir/pdfium/lib/libpdfium.so" | grep -Eq 'Machine:.*X86-64' || fail 'PDFium library has the wrong ELF architecture'

while IFS= read -r -d '' notice; do
    [ -f "$notice" ] && [ ! -L "$notice" ] || fail 'PDFium notice is missing or not a regular file'
done < <(find "$work_dir/pdfium/licenses" -mindepth 1 -print0 2>/dev/null)
[ -n "$(find "$work_dir/pdfium/licenses" -type f -print -quit 2>/dev/null)" ] || fail 'PDFium archive lacks third-party notices'

install -Dm755 "$REPO_ROOT/apps/linux-gtk/package/vitela" "$stage_usr/bin/vitela"
install -Dm755 "$LINUX_GTK_BINARY" "$stage_usr/lib/vitela/linux-gtk"
install -Dm644 "$work_dir/pdfium/lib/libpdfium.so" "$stage_usr/lib/vitela/libpdfium.so"
install -Dm644 "$REPO_ROOT/apps/linux-gtk/package/org.vitela.Pdf.desktop" "$stage_usr/share/applications/org.vitela.Pdf.desktop"
install -Dm644 "$REPO_ROOT/apps/linux-gtk/package/org.vitela.Pdf.svg" "$stage_usr/share/icons/hicolor/scalable/apps/org.vitela.Pdf.svg"
install -Dm644 "$REPO_ROOT/LICENSE-MIT" "$stage_usr/share/doc/vitela/LICENSE-MIT"
install -Dm644 "$REPO_ROOT/LICENSE-APACHE" "$stage_usr/share/doc/vitela/LICENSE-APACHE"
install -Dm644 "$work_dir/pdfium/LICENSE" "$stage_usr/share/doc/vitela/pdfium/LICENSE"
mkdir -p "$stage_usr/share/doc/vitela/pdfium/licenses"
cp -a --no-preserve=mode "$work_dir/pdfium/licenses/." "$stage_usr/share/doc/vitela/pdfium/licenses/"
find "$stage_usr/share/doc/vitela/pdfium/licenses" -type l -print -quit | grep -q . && fail 'PDFium notice symlink escaped staging'

cp -a "$stage_usr/." "$deb_root/usr/"
sed -e "s/@VERSION@/$PACKAGE_VERSION/" -e 's/@DEPENDENCIES@/libgtk-4-1/' \
    "$REPO_ROOT/apps/linux-gtk/package/debian-control.in" > "$deb_root/DEBIAN/control"
dpkg-deb --build --root-owner-group "$deb_root" "$staged_packages/vitela_${PACKAGE_VERSION}_amd64.deb" >/dev/null

app_dir="$work_dir/AppDir"
mkdir -p "$app_dir"
cp -a "$stage_usr/." "$app_dir/usr/"
install -Dm755 "$REPO_ROOT/apps/linux-gtk/package/vitela" "$app_dir/AppRun"
"${LINUXDEPLOY:-linuxdeploy}" --appdir "$app_dir" --executable "$app_dir/usr/lib/vitela/linux-gtk" \
    --desktop-file "$app_dir/usr/share/applications/org.vitela.Pdf.desktop" \
    --icon-file "$app_dir/usr/share/icons/hicolor/scalable/apps/org.vitela.Pdf.svg"
ARCH=x86_64 "${APPIMAGETOOL:-appimagetool}" "$app_dir" "$staged_packages/Vitela-${PACKAGE_VERSION}-x86_64.AppImage"
chmod 755 "$staged_packages/Vitela-${PACKAGE_VERSION}-x86_64.AppImage"
mv "$staged_packages" "$packages_dir"
