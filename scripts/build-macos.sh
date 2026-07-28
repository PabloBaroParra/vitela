#!/usr/bin/env bash
# Builds and validates the unsigned Vitela development application. The
# deployment gate is intentionally fail-closed: a library requiring a newer
# macOS version must be rebuilt or trigger a spec change, never a silent floor
# increase.
set -euo pipefail

# Set by the floor of the pinned PDFium 7763 universal build, which declares
# minos 12.0. Vitela cannot claim a floor its own bundled renderer does not
# meet; lowering this means pinning a different PDFium for every platform.
readonly minimum_macos_version="12.0"

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage:
  scripts/build-macos.sh --assemble
  scripts/build-macos.sh --check-deployment [Mach-O paths...]
  scripts/build-macos.sh --verify-bundle <Vitela.app>
  scripts/build-macos.sh --validate-bundle-layout <Vitela.app>

When no Mach-O paths are supplied, --check-deployment inspects the built app
and its bundled Frameworks directory. These commands require macOS tools when
given real Mach-O files.

--assemble requires PDFIUM_DYLIB to name the pinned libpdfium.dylib.
EOF
}

bundle_layout() {
  local app_path=${1:-}
  [[ -n "$app_path" ]] || die "bundle layout validation requires a .app path"
  [[ -f "$app_path/Contents/MacOS/Vitela" ]] || die "missing application executable"
  # Without Info.plist the directory is not a launchable bundle, however
  # correct its Mach-O slices are — so the gate has to check for it.
  [[ -f "$app_path/Contents/Info.plist" ]] || die "missing Contents/Info.plist"
  [[ -f "$app_path/Contents/Frameworks/libpdf_ffi.dylib" ]] || die "missing bundled libpdf_ffi.dylib"
  [[ -f "$app_path/Contents/Frameworks/libpdfium.dylib" ]] || die "missing bundled libpdfium.dylib"
}

collect_bundle_binaries() {
  local app_path=$1
  local candidate
  while IFS= read -r -d '' candidate; do
    if file -b "$candidate" | grep -q 'Mach-O'; then
      printf '%s\n' "$candidate"
    fi
  done < <(find "$app_path/Contents" -type f -print0)
}

# Regenerates the Swift bindings from the compiled cdylib. Library mode means
# uniffi reads the metadata embedded by `uniffi::setup_scaffolding!()`, so the
# bindings can never drift from the Rust surface actually built.
generate_bindings() {
  local repo_root=$1
  local generated_dir=$2

  mkdir -p "$generated_dir"
  cargo run -p pdf-ffi --features bindgen --locked --bin uniffi-bindgen -- \
    generate --library "$repo_root/target/release/libpdf_ffi.dylib" \
    --language swift --out-dir "$generated_dir"

  local artifact
  for artifact in pdf_ffi.swift pdf_ffiFFI.h pdf_ffiFFI.modulemap; do
    [[ -f "$generated_dir/$artifact" ]] || die "missing generated binding artifact: $artifact"
  done

  # `swiftc -I` only discovers a module map named `module.modulemap`, which is
  # how `pdf_ffi.swift`'s `import pdf_ffiFFI` resolves against the C header.
  cp -f "$generated_dir/pdf_ffiFFI.modulemap" "$generated_dir/module.modulemap"
}

assemble() {
  [[ "$(uname -s)" == "Darwin" ]] || die "--assemble requires macOS"
  local repo_root
  repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
  local generated_dir="$repo_root/apps/macos/Generated"
  local app_path="${VITELA_APP_PATH:-$repo_root/build/Vitela.app}"
  local pdfium_path=${PDFIUM_DYLIB:-}
  [[ -f "$pdfium_path" ]] || die "PDFIUM_DYLIB must name the pinned libpdfium.dylib to bundle"
  export PDFIUM_DYLIB="$pdfium_path"

  mkdir -p "$repo_root/build"
  cargo build -p pdf-ffi --release --locked

  # rustc stamps a cdylib's install name with its bare file name, which dyld
  # would resolve against the working directory. Rewriting it before Xcode
  # links is what makes the app record `@rpath/libpdf_ffi.dylib` and find the
  # copy in Contents/Frameworks.
  install_name_tool -id @rpath/libpdf_ffi.dylib "$repo_root/target/release/libpdf_ffi.dylib"

  generate_bindings "$repo_root" "$generated_dir"

  xcodebuild -project "$repo_root/apps/macos/Vitela.xcodeproj" -scheme Vitela \
    -configuration Release -derivedDataPath "$repo_root/apps/macos/DerivedData" build
  local built_app="$repo_root/apps/macos/DerivedData/Build/Products/Release/Vitela.app"
  [[ -d "$built_app" ]] || die "Xcode did not produce Vitela.app"
  rm -rf "$app_path"
  cp -R "$built_app" "$app_path"

  # Ad-hoc signature, not a distribution one: arm64 refuses to execute an
  # entirely unsigned Mach-O, so without this the "development artifact" could
  # not be opened on any Apple Silicon Mac. Nested code is signed first.
  codesign --force --sign - "$app_path/Contents/Frameworks/libpdf_ffi.dylib"
  codesign --force --sign - "$app_path/Contents/Frameworks/libpdfium.dylib"
  codesign --force --sign - "$app_path"

  bundle_layout "$app_path"
}

version_at_most() {
  local actual=$1
  local maximum=$2
  local actual_major actual_minor maximum_major maximum_minor
  IFS=. read -r actual_major actual_minor _ <<< "$actual"
  IFS=. read -r maximum_major maximum_minor _ <<< "$maximum"
  actual_minor=${actual_minor:-0}
  maximum_minor=${maximum_minor:-0}

  (( actual_major < maximum_major )) || {
    (( actual_major == maximum_major && actual_minor <= maximum_minor ))
  }
}

minos_for() {
  local binary=$1
  local output

  if command -v vtool >/dev/null 2>&1; then
    output=$(vtool -show-build "$binary" 2>&1) || die "unable to inspect Mach-O '$binary' with vtool: $output"
    printf '%s\n' "$output" | sed -nE 's/^[[:space:]]*minos[[:space:]]+([0-9]+(\.[0-9]+){0,2}).*/\1/p'
    return
  fi

  if command -v otool >/dev/null 2>&1; then
    output=$(otool -l "$binary" 2>&1) || die "unable to inspect Mach-O '$binary' with otool: $output"
    printf '%s\n' "$output" | awk '/minos/ { print $2 }'
    return
  fi

  die "macOS deployment inspection requires vtool or otool"
}

check_binary_deployment() {
  local binary=$1
  local minos_values minos inspected=0
  minos_values=$(minos_for "$binary")
  [[ -n "$minos_values" ]] || die "could not determine the deployment target for '$binary'"

  while IFS= read -r minos; do
    [[ -n "$minos" ]] || continue
    inspected=1
    if ! version_at_most "$minos" "$minimum_macos_version"; then
      die "'$binary' requires macOS $minos; Vitela supports macOS $minimum_macos_version"
    fi
    printf 'ok: %s declares macOS %s (maximum allowed: %s)\n' "$binary" "$minos" "$minimum_macos_version"
  done <<< "$minos_values"
  (( inspected == 1 )) || die "could not determine the deployment target for '$binary'"
}

check_deployment() {
  local binaries=("$@")
  if (( ${#binaries[@]} == 0 )); then
    [[ "$(uname -s)" == "Darwin" ]] || die "--check-deployment without explicit fixtures requires macOS"
    local app_path="${VITELA_APP_PATH:-build/Vitela.app}"
    [[ -f "$app_path/Contents/MacOS/Vitela" ]] || die "missing built application executable: $app_path/Contents/MacOS/Vitela"
    bundle_layout "$app_path"
    while IFS= read -r binary; do
      binaries+=("$binary")
    done < <(collect_bundle_binaries "$app_path")
    (( ${#binaries[@]} > 0 )) || die "no Mach-O slices found in $app_path"
  fi

  local binary
  for binary in "${binaries[@]}"; do
    [[ -f "$binary" ]] || die "missing Mach-O input: $binary"
    check_binary_deployment "$binary"
  done
}

verify_bundle() {
  local app_path=${1:-}
  [[ -n "$app_path" ]] || die "--verify-bundle requires a .app path"
  [[ "$(uname -s)" == "Darwin" ]] || die "--verify-bundle requires macOS"
  bundle_layout "$app_path"
  VITELA_APP_PATH="$app_path" check_deployment
}

case ${1:-} in
  --assemble)
    assemble
    ;;
  --check-deployment)
    shift
    check_deployment "$@"
    ;;
  --verify-bundle)
    shift
    verify_bundle "${1:-}"
    ;;
  --validate-bundle-layout)
    shift
    bundle_layout "${1:-}"
    ;;
  --help|-h|"")
    usage
    ;;
  *)
    die "unknown command: $1"
    ;;
esac
