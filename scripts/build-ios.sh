#!/usr/bin/env bash
# iOS packaging commands must fail closed when their declared deployment floor
# drifts: a binary built for a newer iOS version cannot run on the supported
# iOS 15 floor.
#
# Scope, stated plainly: this validates the DECLARED floor only. It reads the
# real Xcode build setting (IPHONEOS_DEPLOYMENT_TARGET) out of the project and
# asserts CI agrees with it. It does NOT inspect a Mach-O, because no iOS
# binary is built yet — there are no sources and no targets. The macOS
# equivalent is stricter on purpose: scripts/build-macos.sh --verify-bundle
# reads `minos` from the actual binary with vtool/otool. Once an iOS target
# produces a binary, this gate must be upgraded the same way.
set -euo pipefail

readonly approved_ios_deployment_floor='15.0'

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
readonly project_file_default="$repo_root/apps/ios/VitelaIOS.xcodeproj/project.pbxproj"
readonly ci_file_default="$repo_root/.github/workflows/ios.yml"

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage:
  scripts/build-ios.sh --validate-deployment-floor
  scripts/build-ios.sh --prepare <iphonesimulator|iphoneos>
  scripts/build-ios.sh --verify-deployment iphoneos

--validate-deployment-floor requires IOS_DEPLOYMENT_FLOOR=15.0 and checks that
the Xcode project's IPHONEOS_DEPLOYMENT_TARGET and the Apple CI workflow declare
that same approved deployment floor.

--prepare builds the Rust FFI for the matching Apple triple and regenerates the
Swift bindings, so `xcodebuild` can link and run the shell. It deliberately does
NOT invoke xcodebuild: the build and the test run stay separate steps so CI
reports which of the two failed.

--verify-deployment reads the actual `minos` load command out of both dylibs the
device build loads at runtime and fails when either requires a newer iOS than
the approved floor. This is the check that makes the floor mean something; the
declaration check only proves the project and CI agree with each other. It is
device-only by design — see the comment above verify_deployment().
EOF
}

# Regenerates the Swift bindings from the compiled host cdylib. Library mode
# means uniffi reads the metadata embedded by `uniffi::setup_scaffolding!()`, so
# the bindings can never drift from the Rust surface actually built. The host
# library is used on purpose: it is the same crate, and uniffi-bindgen has to be
# able to dlopen what it inspects — which it cannot do with an iOS slice.
generate_bindings() {
  local repo_root=$1
  local generated_dir=$2

  mkdir -p "$generated_dir"
  cargo build -p pdf-ffi --release --locked
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

rust_target_for() {
  case $1 in
    iphonesimulator) printf 'aarch64-apple-ios-sim\n' ;;
    iphoneos) printf 'aarch64-apple-ios\n' ;;
    *) die "unsupported iOS platform: '$1' (expected iphonesimulator or iphoneos)" ;;
  esac
}

prepare() {
  local platform=${1:-}
  [[ -n "$platform" ]] || die '--prepare requires a platform (iphonesimulator or iphoneos)'

  # Argument validation before the host check on purpose: a typo'd platform is
  # then rejected identically everywhere, which is what makes it testable off a
  # Mac — the rest of this function cannot be.
  local rust_target
  rust_target=$(rust_target_for "$platform")

  [[ "$(uname -s)" == "Darwin" ]] || die '--prepare requires macOS'

  local generated_dir="$repo_root/apps/ios/Generated"
  local pdfium_path=${PDFIUM_DYLIB:-"$repo_root/core/pdf-render/vendor/pdfium-$platform/lib/libpdfium.dylib"}
  # Checked here rather than only in the Xcode build phase, so a missing PDFium
  # fails before a ten-minute compile instead of after it.
  [[ -f "$pdfium_path" ]] || die "no libpdfium.dylib at '$pdfium_path' — set PDFIUM_DYLIB to the pinned $platform build"

  rustup target add "$rust_target"
  # rustc otherwise picks its own (much older) default deployment target, so the
  # FFI would declare a floor nobody chose — it came out as iOS 14.0 on the
  # simulator slice. Pinning it here makes the Rust library declare exactly the
  # floor the Xcode project and CI already agree on.
  IPHONEOS_DEPLOYMENT_TARGET="$approved_ios_deployment_floor" \
    cargo build -p pdf-ffi --release --locked --target "$rust_target"

  local ffi="$repo_root/target/$rust_target/release/libpdf_ffi.dylib"
  [[ -f "$ffi" ]] || die "cargo did not produce $ffi"

  # rustc stamps a cdylib's install name with its bare file name, which dyld
  # would resolve against the working directory. Rewriting it before Xcode links
  # is what makes the app record `@rpath/libpdf_ffi.dylib` and find the copy in
  # the bundle's Frameworks directory.
  install_name_tool -id @rpath/libpdf_ffi.dylib "$ffi"

  generate_bindings "$repo_root" "$generated_dir"

  printf 'ok: prepared %s (%s) — FFI at %s, bindings in %s\n' \
    "$platform" "$rust_target" "$ffi" "$generated_dir"
}

# Extracts every declaration of the floor and requires them all to agree.
# Matching per-occurrence rather than per-line is not a detail: Xcode writes a
# target's entire `buildSettings` dictionary on one line, so a line-anchored
# pattern silently finds nothing. Collapsing with `sort -u` also means a project
# where one target drifted is reported as ambiguous instead of passing on
# whichever declaration happened to come first.
declared_floor() {
  local file=$1
  local key_pattern=$2
  local source_name=$3
  local values count

  [[ -f "$file" ]] || die "missing $source_name deployment-floor source: $file"
  values=$(grep -oE "$key_pattern" "$file" | sed -E 's/.*[:=][[:space:]]*"?//; s/"$//' | sort -u)
  [[ -n "$values" ]] || die "missing iOS deployment floor in $source_name: $file"

  count=$(printf '%s\n' "$values" | wc -l)
  [[ "$count" -eq 1 ]] || die "ambiguous iOS deployment floor in $source_name ($file): $(printf '%s' "$values" | tr '\n' ' ')"
  printf '%s\n' "$values"
}

validate_deployment_floor() {
  local environment_floor=${IOS_DEPLOYMENT_FLOOR:-}
  local project_file=${IOS_PROJECT_FILE:-$project_file_default}
  local ci_file=${IOS_CI_FILE:-$ci_file_default}
  local project_floor ci_floor

  [[ -n "$environment_floor" ]] || die 'IOS_DEPLOYMENT_FLOOR is required'
  [[ "$environment_floor" == "$approved_ios_deployment_floor" ]] || die "IOS_DEPLOYMENT_FLOOR must be exactly $approved_ios_deployment_floor; got '$environment_floor'"

  project_floor=$(declared_floor "$project_file" 'IPHONEOS_DEPLOYMENT_TARGET[[:space:]]*=[[:space:]]*[0-9]+(\.[0-9]+)?' 'Xcode project')
  [[ "$project_floor" == "$approved_ios_deployment_floor" ]] || die "iOS deployment floor mismatch: project declares $project_floor, expected $approved_ios_deployment_floor"

  ci_floor=$(declared_floor "$ci_file" 'IOS_DEPLOYMENT_FLOOR:[[:space:]]*"?[0-9]+(\.[0-9]+)?"?' 'Apple CI workflow')
  [[ "$ci_floor" == "$approved_ios_deployment_floor" ]] || die "iOS deployment floor mismatch: CI declares $ci_floor, expected $approved_ios_deployment_floor"

  printf 'ok: iOS deployment floor is %s in environment, project, and CI\n' "$approved_ios_deployment_floor"
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

# Reads every declared iOS deployment version out of a Mach-O.
#
# Two load commands have to be understood, not one. A modern toolchain emits
# LC_BUILD_VERSION with a `minos` field, but a low deployment target still
# produces the older LC_VERSION_MIN_IPHONEOS with a `version` field — which is
# exactly what rustc's default for aarch64-apple-ios does. Reading only `minos`
# made the gate report "could not determine the deployment target" on a binary
# that was declaring one perfectly clearly.
#
# The `cmd` tracking matters: a bare `version` match would also pick up the
# `current version` / `compatibility version` fields of LC_ID_DYLIB, which have
# nothing to do with deployment.
extract_ios_versions() {
  awk '
    $1 == "cmd" { command = $2 }
    $1 == "minos" { print $2 }
    $1 == "version" && command == "LC_VERSION_MIN_IPHONEOS" { print $2 }
  '
}

minos_for() {
  local binary=$1
  local output

  if command -v vtool >/dev/null 2>&1; then
    output=$(vtool -show-build "$binary" 2>&1) || die "unable to inspect Mach-O '$binary' with vtool: $output"
    printf '%s\n' "$output" | extract_ios_versions
    return
  fi

  if command -v otool >/dev/null 2>&1; then
    output=$(otool -l "$binary" 2>&1) || die "unable to inspect Mach-O '$binary' with otool: $output"
    printf '%s\n' "$output" | extract_ios_versions
    return
  fi

  die "iOS deployment inspection requires vtool or otool"
}

check_binary_deployment() {
  local binary=$1
  local minos_values minos

  [[ -f "$binary" ]] || die "cannot inspect a missing binary: $binary"
  minos_values=$(minos_for "$binary")
  [[ -n "$minos_values" ]] || die "could not determine the deployment target for '$binary'"

  while IFS= read -r minos; do
    [[ -n "$minos" ]] || continue
    if ! version_at_most "$minos" "$approved_ios_deployment_floor"; then
      die "'$binary' requires iOS $minos; Vitela supports iOS $approved_ios_deployment_floor"
    fi
    printf 'ok: %s declares iOS %s (maximum allowed: %s)\n' "$binary" "$minos" "$approved_ios_deployment_floor"
  done <<< "$minos_values"
}

# The real gate, and the reason the declaration check above is not the end of
# it: a build setting is a claim, a Mach-O load command is a fact. Both dylibs
# the app loads at runtime are inspected, because the app can only claim the
# floor its bundled renderer actually supports.
#
# Device only, on purpose. The deployment floor is a promise about which
# iPhones can run Vitela, and only the device slice is ever shipped. A
# simulator slice's `minos` says which *simulator runtimes* can load it — the
# pinned PDFium 7763 simulator build declares iOS 26.0 while its device build
# is what the floor is actually about. Gating the product floor on the
# simulator slice conflates a CI-machine requirement with a product claim; the
# simulator is instead proven by the test run itself, which cannot pass unless
# the library loads.
verify_deployment() {
  local platform=${1:-}
  [[ -n "$platform" ]] || die '--verify-deployment requires a platform (iphoneos)'
  [[ "$platform" == "iphoneos" ]] || die "--verify-deployment applies to the shipped device slice; got '$platform'"

  local rust_target
  rust_target=$(rust_target_for "$platform")

  local pdfium_path=${PDFIUM_DYLIB:-"$repo_root/core/pdf-render/vendor/pdfium-$platform/lib/libpdfium.dylib"}
  check_binary_deployment "$repo_root/target/$rust_target/release/libpdf_ffi.dylib"
  check_binary_deployment "$pdfium_path"
}

case ${1:-} in
  --validate-deployment-floor)
    validate_deployment_floor
    ;;
  --prepare)
    prepare "${2:-}"
    ;;
  --verify-deployment)
    verify_deployment "${2:-}"
    ;;
  --help|-h|'')
    usage
    ;;
  *)
    die "unknown command: $1"
    ;;
esac
