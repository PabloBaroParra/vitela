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

require_command cargo
require_command cargo-ndk

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

library="$jni_root/arm64-v8a/libpdf_ffi.so"
require_file "$library" "cargo-ndk output"
cargo run -p pdf-ffi --features bindgen --bin uniffi-bindgen -- generate --library "$library" --language kotlin --out-dir "$generated_root"

adapter_root="$generated_root/dev/vitela/pdf/generated"
mkdir -p "$adapter_root" "$resources_root/META-INF/services"
cp "$android_root/GeneratedPdfCore.kt.template" "$adapter_root/GeneratedPdfCore.kt"
printf 'dev.vitela.pdf.generated.GeneratedPdfCoreFactory\n' > "$resources_root/META-INF/services/dev.vitela.pdf.core.PdfCoreFactory"
popd >/dev/null

echo "Android native libraries and matching UniFFI Kotlin bindings are ready."
