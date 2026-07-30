#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
android_root="$repository_root/apps/android"
app_root="$android_root/app"
generated_root="$app_root/build/generated/uniffi/kotlin"
# Java resources live in their own tree, NOT alongside the generated sources.
# A Gradle `java.srcDir` is compiled, never packaged, so a META-INF/services
# descriptor placed there silently never reaches the APK and the ServiceLoader
# finds no PdfCoreFactory at runtime. Only this directory is registered as a
# `resources.srcDir`, which also keeps the generated .kt files out of the APK.
resources_root="$app_root/build/generated/uniffi/resources"
jni_root="$app_root/src/main/jniLibs"
abis=(arm64-v8a x86_64)

require_command() {
    command -v "$1" >/dev/null || { echo "Required command not found: $1" >&2; exit 1; }
}

require_file() {
    [[ -f "$1" ]] || { echo "Missing $2: $1" >&2; exit 1; }
}

find_llvm_readelf() {
    local ndk_home="${ANDROID_NDK_HOME:-}"
    local candidate
    local candidates=()

    if [[ -z "$ndk_home" ]]; then
        echo "ANDROID_NDK_HOME must point to an Android NDK containing llvm-readelf." >&2
        exit 1
    fi

    shopt -s nullglob
    candidates=(
        "$ndk_home"/toolchains/llvm/prebuilt/*/bin/llvm-readelf
        "$ndk_home"/toolchains/llvm/prebuilt/*/bin/llvm-readelf.exe
    )
    shopt -u nullglob

    for candidate in "${candidates[@]}"; do
        if [[ -f "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return
        fi
    done

    echo "Could not find llvm-readelf under ANDROID_NDK_HOME: $ndk_home" >&2
    echo "Expected toolchains/llvm/prebuilt/<host>/bin/llvm-readelf." >&2
    exit 1
}

require_16kb_load_alignment() {
    local library="$1"
    local description="$2"
    local alignment
    local found_load=false
    local all_loads_16kb_aligned=true
    local load_alignments

    require_file "$library" "$description"
    load_alignments=$("$llvm_readelf" -lW "$library" | awk '$1 == "LOAD" { print $NF }')

    while IFS= read -r alignment; do
        [[ -n "$alignment" ]] || continue
        found_load=true
        if ! [[ "$alignment" =~ ^0[xX][0-9A-Fa-f]+$ ]] || ((16#${alignment:2} < 0x4000)); then
            all_loads_16kb_aligned=false
        fi
    done <<< "$load_alignments"

    if [[ "$found_load" != true || "$all_loads_16kb_aligned" != true ]]; then
        echo "16-KB ELF alignment check failed for $description: $library" >&2
        echo "Every Android native library must have a PT_LOAD alignment of at least 0x4000." >&2
        echo "PT_LOAD segments reported by $llvm_readelf:" >&2
        "$llvm_readelf" -lW "$library" | awk '$1 == "LOAD" { print }' >&2
        echo "Rebuild this library with 16-KB page-size support before running Gradle." >&2
        exit 1
    fi

    printf 'Verified 16-KB PT_LOAD alignment for %s: %s\n' "$description" "$library"
}

require_command cargo
require_command cargo-ndk
llvm_readelf="$(find_llvm_readelf)"

for abi in "${abis[@]}"; do
    variable="PDFIUM_ANDROID_${abi^^}"
    variable="${variable//-/_}"
    pdfium_path="${!variable:-}"
    if [[ -z "$pdfium_path" ]]; then
        echo "Missing $variable. Set it to an externally obtained libpdfium.so for $abi." >&2
        echo "PDFium is not vendored or downloaded by this repository. See apps/android/README.md." >&2
        exit 1
    fi
    require_file "$pdfium_path" "$variable"
done

rm -rf "$generated_root" "$resources_root" "$jni_root"
mkdir -p "$generated_root" "$resources_root" "$jni_root"

pushd "$repository_root" >/dev/null
cargo ndk -t arm64-v8a -t x86_64 -o "$jni_root" build -p pdf-ffi --release

for abi in "${abis[@]}"; do
    variable="PDFIUM_ANDROID_${abi^^}"
    variable="${variable//-/_}"
    mkdir -p "$jni_root/$abi"
    cp "${!variable}" "$jni_root/$abi/libpdfium.so"
done

for abi in "${abis[@]}"; do
    require_16kb_load_alignment "$jni_root/$abi/libpdf_ffi.so" "cargo-ndk libpdf_ffi.so for $abi"
    require_16kb_load_alignment "$jni_root/$abi/libpdfium.so" "supplied libpdfium.so for $abi"
done

library="$jni_root/arm64-v8a/libpdf_ffi.so"
cargo run -p pdf-ffi --features bindgen --bin uniffi-bindgen -- generate --library "$library" --language kotlin --out-dir "$generated_root"

adapter_root="$generated_root/dev/vitela/pdf/generated"
mkdir -p "$adapter_root" "$resources_root/META-INF/services"
cp "$android_root/GeneratedPdfCore.kt.template" "$adapter_root/GeneratedPdfCore.kt"
printf 'dev.vitela.pdf.generated.GeneratedPdfCoreFactory\n' > "$resources_root/META-INF/services/dev.vitela.pdf.core.PdfCoreFactory"
popd >/dev/null

echo "Android native libraries and matching UniFFI Kotlin bindings are ready."
