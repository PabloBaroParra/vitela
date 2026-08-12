#!/usr/bin/env bash
set -euo pipefail

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
readonly PACKAGE_SCRIPT="$REPO_ROOT/scripts/package-linux.sh"

fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
assert_file() { [ -f "$1" ] || fail "expected file: $1"; }
assert_not_exists() { [ ! -e "$1" ] || fail "unexpected path: $1"; }
assert_no_artifacts() {
    [ ! -d "$1" ] || ! find "$1" -maxdepth 1 -type f \( -name '*.deb' -o -name '*.AppImage' \) -print -quit | grep -q . || fail "unexpected published artifact in $1"
}
assert_contains() { grep -Fqx -- "$2" "$1" || fail "expected $2 in $1"; }

fixture_root=''
cleanup() { [ -z "$fixture_root" ] || rm -rf -- "$fixture_root"; }
trap cleanup EXIT

make_fixture() {
    fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/vitela package.XXXXXX")"
    mkdir -p "$fixture_root/input/lib" "$fixture_root/input/licenses" "$fixture_root/bin"
    printf '%s\n' 'MAJOR=148' 'MINOR=0' 'BUILD=7763' 'PATCH=0' > "$fixture_root/input/VERSION"
    printf '%s\n' 'target_os="linux" target_cpu="x64" pdf_use_v8=false pdf_enable_xfa=false' > "$fixture_root/input/args.gn"
    printf '%s\n' 'PDFium license' > "$fixture_root/input/LICENSE"
    printf '%s\n' 'third-party notice' > "$fixture_root/input/licenses/pdfium.txt"
    cp /bin/true "$fixture_root/input/lib/libpdfium.so"
    printf '%s\n' '#!/usr/bin/env bash' 'exit 0' > "$fixture_root/linux-gtk"
    chmod +x "$fixture_root/linux-gtk"
    printf '%s\n' '#!/usr/bin/env bash' 'exit 0' > "$fixture_root/bin/linuxdeploy"
    cat > "$fixture_root/bin/appimagetool" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
source_dir="$1"
destination="${!#}"
cp -a "$source_dir" "$destination.payload"
cat > "$destination" <<'APPIMAGE'
#!/usr/bin/env bash
set -euo pipefail
payload="$0.payload"
case "${1:-}" in
    --appimage-extract) cp -a "$payload" squashfs-root ;;
    --appimage-extract-and-run) shift; APPDIR="$payload" exec "$payload/AppRun" "$@" ;;
    *) exit 64 ;;
esac
APPIMAGE
chmod +x "$destination"
EOF
    chmod +x "$fixture_root/bin/linuxdeploy" "$fixture_root/bin/appimagetool"
    tar -C "$fixture_root/input" -czf "$fixture_root/pdfium-linux-x64.tgz" .
    sha256sum "$fixture_root/pdfium-linux-x64.tgz" | awk '{print $1}' > "$fixture_root/sha256"
}

run_package() {
    PATH="$fixture_root/bin:$PATH" \
    PDFIUM_ARCHIVE="$fixture_root/pdfium-linux-x64.tgz" \
    PDFIUM_SHA256="$(cat "$fixture_root/sha256")" \
    LINUX_GTK_BINARY="${LINUX_GTK_BINARY_OVERRIDE:-$fixture_root/linux-gtk}" \
    PACKAGE_BUILD_ROOT="$fixture_root/build output;not-a-command" \
    "$PACKAGE_SCRIPT"
}

test_missing_or_untrusted_input_fails_before_publication() {
    make_fixture
    rm "$fixture_root/pdfium-linux-x64.tgz"
    if run_package; then fail 'missing archive unexpectedly packaged'; fi
    assert_no_artifacts "$fixture_root/build output;not-a-command/packages"
}

test_checksum_metadata_and_library_validation_fail_closed() {
    make_fixture
    printf '%s\n' 'wrong-checksum' > "$fixture_root/sha256"
    if run_package; then fail 'checksum mismatch unexpectedly packaged'; fi
    assert_no_artifacts "$fixture_root/build output;not-a-command/packages"
}

test_archive_with_many_entries_after_library_is_accepted() {
    make_fixture
    mkdir -p "$fixture_root/input/trailing"
    local index
    for index in $(seq 1 10000); do
        : > "$fixture_root/input/trailing/$index"
    done
    tar -C "$fixture_root/input" -czf "$fixture_root/pdfium-linux-x64.tgz" .
    sha256sum "$fixture_root/pdfium-linux-x64.tgz" | awk '{print $1}' > "$fixture_root/sha256"
    run_package
    assert_file "$fixture_root/build output;not-a-command/packages/vitela_$(awk -F'"' '/^version = / { print $2; exit }' "$REPO_ROOT/Cargo.toml")_amd64.deb"
}

test_version_metadata_and_missing_tools_fail_before_publication() {
    make_fixture
    printf '%s\n' 'MAJOR=148' 'MINOR=0' 'BUILD=0000' 'PATCH=0' > "$fixture_root/input/VERSION"
    tar -C "$fixture_root/input" -czf "$fixture_root/pdfium-linux-x64.tgz" .
    sha256sum "$fixture_root/pdfium-linux-x64.tgz" | awk '{print $1}' > "$fixture_root/sha256"
    if run_package; then fail 'wrong version unexpectedly packaged'; fi
    assert_no_artifacts "$fixture_root/build output;not-a-command/packages"

    printf '%s\n' '148.0.7763.0' > "$fixture_root/input/VERSION"
    tar -C "$fixture_root/input" -czf "$fixture_root/pdfium-linux-x64.tgz" .
    sha256sum "$fixture_root/pdfium-linux-x64.tgz" | awk '{print $1}' > "$fixture_root/sha256"
    if run_package; then fail 'malformed VERSION metadata unexpectedly packaged'; fi
    assert_no_artifacts "$fixture_root/build output;not-a-command/packages"

    printf '%s\n' 'MAJOR=148' 'MINOR=0' 'BUILD=7763' 'PATCH=0' > "$fixture_root/input/VERSION"
    tar -C "$fixture_root/input" -czf "$fixture_root/pdfium-linux-x64.tgz" .
    sha256sum "$fixture_root/pdfium-linux-x64.tgz" | awk '{print $1}' > "$fixture_root/sha256"
    if LINUXDEPLOY=vitela-tool-that-does-not-exist run_package; then fail 'missing tool unexpectedly packaged'; fi
    assert_no_artifacts "$fixture_root/build output;not-a-command/packages"
}

test_v8_x64_and_elf_mismatches_fail_before_publication() {
    make_fixture
    printf '%s\n' 'target_os="linux" target_cpu="x64" pdf_use_v8=true pdf_enable_xfa=false' > "$fixture_root/input/args.gn"
    tar -C "$fixture_root/input" -czf "$fixture_root/pdfium-linux-x64.tgz" .
    sha256sum "$fixture_root/pdfium-linux-x64.tgz" | awk '{print $1}' > "$fixture_root/sha256"
    if run_package; then fail 'V8-enabled archive unexpectedly packaged'; fi
    assert_no_artifacts "$fixture_root/build output;not-a-command/packages"

    printf '%s\n' 'target_os="linux" target_cpu="arm64" pdf_use_v8=false pdf_enable_xfa=false' > "$fixture_root/input/args.gn"
    tar -C "$fixture_root/input" -czf "$fixture_root/pdfium-linux-x64.tgz" .
    sha256sum "$fixture_root/pdfium-linux-x64.tgz" | awk '{print $1}' > "$fixture_root/sha256"
    if run_package; then fail 'non-x64 archive unexpectedly packaged'; fi
    assert_no_artifacts "$fixture_root/build output;not-a-command/packages"

    printf '%s\n' 'target_os="linux" target_cpu="x64" pdf_use_v8=false pdf_enable_xfa=false' > "$fixture_root/input/args.gn"
    printf '%s\n' 'not an ELF library' > "$fixture_root/input/lib/libpdfium.so"
    tar -C "$fixture_root/input" -czf "$fixture_root/pdfium-linux-x64.tgz" .
    sha256sum "$fixture_root/pdfium-linux-x64.tgz" | awk '{print $1}' > "$fixture_root/sha256"
    if run_package; then fail 'non-ELF archive unexpectedly packaged'; fi
    assert_no_artifacts "$fixture_root/build output;not-a-command/packages"
}

test_nonzero_child_failure_propagates_without_publication() {
    make_fixture
    printf '%s\n' '#!/usr/bin/env bash' 'exit 37' > "$fixture_root/bin/linuxdeploy"
    chmod +x "$fixture_root/bin/linuxdeploy"
    if run_package; then fail 'failed linuxdeploy unexpectedly packaged'; fi
    assert_no_artifacts "$fixture_root/build output;not-a-command/packages"
}

test_deb_inspector_fixture_has_the_required_payload() {
    make_fixture
    run_package
    assert_file "$fixture_root/build output;not-a-command/packages/vitela_$(awk -F'"' '/^version = / { print $2; exit }' "$REPO_ROOT/Cargo.toml")_amd64.deb"
    assert_file "$fixture_root/build output;not-a-command/packages/Vitela-$(awk -F'"' '/^version = / { print $2; exit }' "$REPO_ROOT/Cargo.toml")-x86_64.AppImage"
}

test_artifact_inspectors_reject_bad_metadata_entries_and_appimage_payload() {
    make_fixture
    LINUX_GTK_BINARY_OVERRIDE=/bin/true run_package
    local packages="$fixture_root/build output;not-a-command/packages"
    local evidence="$fixture_root/evidence"
    if ! PACKAGE_EVIDENCE_DIR="$evidence" VERIFY_INSPECT_ONLY=1 "$REPO_ROOT/scripts/verify-linux-package.sh" "$packages"; then
        fail 'valid fixture artifacts did not pass inspectors'
    fi

    local deb_root="$fixture_root/bad-deb"
    mkdir -p "$deb_root/DEBIAN"
    printf '%s\n' 'Package: vitela' 'Version: 0.1.0' 'Architecture: arm64' 'Description: invalid architecture' > "$deb_root/DEBIAN/control"
    dpkg-deb --build "$deb_root" "$packages/vitela_0.1.0_amd64.deb" >/dev/null
    if PACKAGE_EVIDENCE_DIR="$evidence" VERIFY_INSPECT_ONLY=1 "$REPO_ROOT/scripts/verify-linux-package.sh" "$packages"; then
        fail 'wrong-architecture deb unexpectedly passed inspection'
    fi

    LINUX_GTK_BINARY_OVERRIDE=/bin/true run_package
    rm "$packages/Vitela-0.1.0-x86_64.AppImage.payload/usr/lib/vitela/libpdfium.so"
    if PACKAGE_EVIDENCE_DIR="$evidence" VERIFY_INSPECT_ONLY=1 "$REPO_ROOT/scripts/verify-linux-package.sh" "$packages"; then
        fail 'AppImage missing private library unexpectedly passed inspection'
    fi
}

test_notices_are_data_and_required_notice_symlinks_fail() {
    make_fixture
    printf '%s\n' '#!/usr/bin/env bash' 'exit 99' > "$fixture_root/input/README.sh"
    chmod +x "$fixture_root/input/README.sh"
    ln -s pdfium.txt "$fixture_root/input/licenses/linked-notice.txt"
    tar -C "$fixture_root/input" -czf "$fixture_root/pdfium-linux-x64.tgz" .
    sha256sum "$fixture_root/pdfium-linux-x64.tgz" | awk '{print $1}' > "$fixture_root/sha256"
    if run_package; then fail 'notice symlink unexpectedly packaged'; fi
    assert_no_artifacts "$fixture_root/build output;not-a-command/packages"
}

test_launcher_rejects_missing_private_library_and_preserves_arguments() {
    make_fixture
    local prefix="$fixture_root/installed prefix"
    mkdir -p "$prefix/bin" "$prefix/lib/vitela"
    cp "$REPO_ROOT/apps/linux-gtk/package/vitela" "$prefix/bin/vitela"
    printf '%s\n' '#!/usr/bin/env bash' 'printf "%s\n" "$PDFIUM_DYNAMIC_LIB_PATH|$*"' > "$prefix/lib/vitela/linux-gtk"
    chmod +x "$prefix/bin/vitela" "$prefix/lib/vitela/linux-gtk"
    if "$prefix/bin/vitela" 'space arg' ';not-a-command'; then fail 'missing library unexpectedly launched'; fi
    printf '%s\n' 'private library' > "$prefix/lib/vitela/libpdfium.so"
    local output
    output="$(env -u PDFIUM_DYNAMIC_LIB_PATH "$prefix/bin/vitela" 'space arg' ';not-a-command')"
    [ "$output" = "$prefix/lib/vitela/libpdfium.so|space arg ;not-a-command" ] || fail "launcher lost path or arguments: $output"
}

test_launcher_uses_appdir_usr_for_relocated_appimages() {
    make_fixture
    local appdir="$fixture_root/relocated AppDir"
    mkdir -p "$appdir/usr/lib/vitela"
    cp "$REPO_ROOT/apps/linux-gtk/package/vitela" "$appdir/AppRun"
    printf '%s\n' '#!/usr/bin/env bash' 'printf "%s\n" "$PDFIUM_DYNAMIC_LIB_PATH|$*"' > "$appdir/usr/lib/vitela/linux-gtk"
    printf '%s\n' 'private library' > "$appdir/usr/lib/vitela/libpdfium.so"
    chmod +x "$appdir/AppRun" "$appdir/usr/lib/vitela/linux-gtk"
    local output
    output="$(env -u PDFIUM_DYNAMIC_LIB_PATH APPDIR="$appdir" "$appdir/AppRun" 'relocated arg')"
    [ "$output" = "$appdir/usr/lib/vitela/libpdfium.so|relocated arg" ] || fail "launcher did not select APPDIR/usr: $output"
}

test_required_assets_exist() {
    assert_file "$REPO_ROOT/scripts/package-linux.sh"
    assert_file "$REPO_ROOT/scripts/verify-linux-package.sh"
    assert_file "$REPO_ROOT/apps/linux-gtk/package/org.vitela.Pdf.desktop"
    assert_file "$REPO_ROOT/apps/linux-gtk/package/org.vitela.Pdf.svg"
    assert_file "$REPO_ROOT/apps/linux-gtk/package/debian-control.in"
}

test_missing_or_untrusted_input_fails_before_publication
test_checksum_metadata_and_library_validation_fail_closed
test_archive_with_many_entries_after_library_is_accepted
test_version_metadata_and_missing_tools_fail_before_publication
test_v8_x64_and_elf_mismatches_fail_before_publication
test_nonzero_child_failure_propagates_without_publication
test_notices_are_data_and_required_notice_symlinks_fail
test_launcher_rejects_missing_private_library_and_preserves_arguments
test_launcher_uses_appdir_usr_for_relocated_appimages
test_deb_inspector_fixture_has_the_required_payload
test_artifact_inspectors_reject_bad_metadata_entries_and_appimage_payload
test_required_assets_exist
printf 'package-linux shell tests: 12 passed\n'
