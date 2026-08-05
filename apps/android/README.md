# Android shell

This Jetpack Compose baseline opens a PDF with the Storage Access Framework,
retries password-protected opens, reads the document as one continuously
scrolling page list, searches text, and delegates printing of the selected
source PDF to Android's print framework. It uses the byte-oriented `pdf-ffi`
UniFFI API exclusively.

## Continuous reader

Every page of the document is a slot in a single `LazyColumn`
([`viewer/PageList.kt`](app/src/main/kotlin/dev/vitela/pdf/viewer/PageList.kt)),
but only a window around the viewport is ever rasterized. Two properties make
that safe on a phone:

- **Slots are laid out before they are rendered.** `PdfDocument.pageSizes`
  reports every page's media box up front, so an unrendered slot already
  occupies its correct height. Without it the list would resize under the
  user's thumb each time a render landed, and scrolling would fight itself.
- **The resident bitmap set is bounded.** A full-width page is several MB as
  ARGB_8888, so decoding a whole document is an OOM, not a slow path.
  `PREFETCH_PAGES` decides what is rasterized ahead of the scroll and
  `CACHE_PAGES` decides what survives once it scrolls off — at most
  `visible + 2 * CACHE_PAGES` pages are held at once
  ([`viewer/PageWindow.kt`](app/src/main/kotlin/dev/vitela/pdf/viewer/PageWindow.kt)).
  Raising either constant multiplies peak heap by the page size.

`ViewerViewModel.onVisibleRangeChanged` is the only thing that triggers a
render, which is what keeps that bound honest; a render that finishes after
its page scrolled out of the cache window is dropped rather than cached. This
mirrors the GTK4 shell's viewport tick (`apps/linux-gtk/src/app/render.rs`).

Previous/Next and search navigation do not swap a page — they set a scroll
target the list animates to, so the reader keeps one scroll position. The page
counter reports the page covering most of the viewport, not the first one
visible: at the bottom of a document the previous page keeps a sliver on
screen, so "first visible" could never say "3 of 3".

## Fit-to-width and button zoom

Pages rasterize at the density that makes them exactly fill their slot —
`renderDpi(pageSize, viewportWidthPx, zoomFactor)` — instead of a fixed DPI, so
a page is sharp on a phone without being wastefully oversized on a small screen.

Two consequences the code has to handle, and both are easy to get wrong:

- **Density must not scale peak heap with the display.** A Letter page across
  a phone's 1080 px is ~1.5 Mpx (6 MB); unfolded across 2560 px it is ~8.5 Mpx
  (34 MB), and `CACHE_PAGES` of those at once is an OOM on a device that is
  not otherwise short of memory. `MAX_PAGE_PIXELS` caps the raster at ~16 MB
  per page, trading a little sharpness on very wide viewports for a bound that
  holds on every device. A degenerate MediaBox is separately clamped by
  `MAX_RENDER_DPI`, mirroring the GTK shell.
- **A layout change keeps a bounded visual bridge.** Each bitmap is tied to the
  width and zoom it was fit to, so a layout change starts a new generation and
  invalidates every render already in flight. Cached pages in the existing
  cache window remain underneath the sharp replacement, avoiding a blank or
  spinner frame; they are removed when replaced or evicted with the same
  window. `ViewerViewModel.layoutGeneration` is how a finished render knows it
  is answering a question nobody is asking any more.

The reader starts at 100% fit-to-width. Its accessible **Zoom out** and
**Zoom in** controls move through this bounded custom scale: 10%, 25%, 50%,
75%, 100%, 125%, 150%, 200%, 300%, 400%, 600%, and 800%. The visible percentage
reports the active level. Zoom changes page-slot geometry and starts sharp
 replacements at the scaled DPI, retaining the previous full-page bitmap only
 as that temporary bridge. It does not use `graphicsLayer` bitmap scaling or
 region tiles. Pages wider than the viewport use one horizontal scroll position
 for the continuous reader.

Pinch gestures and fit-page remain out of scope — see T-084.

## Annotation editing and save copies

When a document permits annotation editing, the reader exposes highlight,
underline, strikeout, ink, shape, text-note, and image-stamp tools. Selecting
an existing annotation enables move, resize, delete, and supported color
restyling actions. A tap selects or places; an editing drag moves, resizes, or
draws, while an unselected pointer drag remains available to scroll the page
list. Image stamps are picked through SAF and use the core's placement policy,
so Android does not invent its own aspect-ratio or anchor rules.

All annotation mutations, undo/redo, byte snapshots, and document replacement
are serialized by `ViewerViewModel`. **Save copy** writes a complete annotated
snapshot through SAF's create-document flow. The dirty state is cleared only if
that write reports success for the same document revision; an older save cannot
clear edits made while it was being written. Opening another document while the
current one is dirty requires confirmation before replacement.

**Open sample** loads the shared sample document instead of going through the
picker. The file is not stored in this module: `app/build.gradle.kts` adds the
repository's `assets/sample/` directory as an asset source, so the APK ships
the byte-identical file the Windows and Linux shells package (see
[`assets/README.md`](../../assets/README.md)). It still needs PDFium to render,
exactly like a picked file.

## Native prerequisite

PDFium is an external runtime prerequisite. This repository does **not** vendor,
download, or claim to distribute PDFium Android binaries. Obtain compatible
non-V8 `libpdfium.so` files yourself, with their required license notices, for
each ABI the app packages. The required release must match the `pdfium_7763`
feature selected by `core/pdf-render/Cargo.toml`.

The packaging script needs the Android NDK (`ANDROID_NDK_HOME`), `cargo-ndk`,
the `aarch64-linux-android` / `x86_64-linux-android` Rust targets, and both
PDFium paths:

```sh
export ANDROID_NDK_HOME=/absolute/path/to/Android/Sdk/ndk/<version>
export PDFIUM_ANDROID_ARM64_V8A=/absolute/path/to/arm64-v8a/libpdfium.so
export PDFIUM_ANDROID_X86_64=/absolute/path/to/x86_64/libpdfium.so
bash scripts/package-android.sh
```

Verify the PDFium build before packaging: its `VERSION` file must report
`BUILD=7763` (matching the `pdfium_7763` feature) and its `args.gn` must have
`pdf_enable_v8 = false`. A mismatched build fails at runtime, not at build
time.

Android distribution APKs must support 16-KB memory pages. Every packaged
`arm64-v8a` and `x86_64` native library needs a `PT_LOAD` alignment of at least
`0x4000`. `package-android.sh` finds `llvm-readelf` from `ANDROID_NDK_HOME` and
fails before Gradle if either generated `libpdf_ffi.so` or supplied
`libpdfium.so` does not meet that requirement. This check also applies to the
actual external PDFium binary, not only its build configuration.

It builds `pdf-ffi` with `cargo-ndk`, copies each externally supplied PDFium
library into `app/src/main/jniLibs/<abi>/`, and generates matching Kotlin
bindings from that exact `libpdf_ffi.so`. The copied libraries and generated
bindings are local build artifacts, ignored by Git.

The script writes two separate generated trees, and the split matters:
`build/generated/uniffi/kotlin` (registered as a `java.srcDir`) holds the
bindings and the `GeneratedPdfCore.kt` adapter, while
`build/generated/uniffi/resources` (registered as a `resources.srcDir`) holds
only the `META-INF/services/dev.vitela.pdf.core.PdfCoreFactory` descriptor.
A Gradle source directory is compiled but never packaged, so a descriptor
placed under the sources tree never reaches the APK — the app then reports
native support as missing even with every `.so` correctly packaged. Do not run the app without
this step: it will show an explicit native-support packaging error instead of
pretending PDF rendering is available.

Without the native core the app also **disables both open actions** ("Open PDF"
and "Open sample"). `ViewerViewModel.open` returns immediately when no
`PdfCoreFactory` is registered, so leaving the buttons enabled would accept
taps it silently drops — `ViewerState.canOpen` keeps the UI honest about what
it can actually do. This applies to the packaged sample too: it is a real PDF
that still needs PDFium to render.

After packaging, use an installed Android Gradle distribution:

```sh
gradle -p apps/android :app:assembleDebug
```

Before distributing an APK, verify both ELF and APK alignment. The packaging
script performs the ELF verification; its output names every checked library.
Then use the Android SDK Build Tools `zipalign` on the release APK:

```sh
zipalign -c -P 16 -v 4 apps/android/app/build/outputs/apk/release/app-release.apk
```

The command must succeed. Do not use compressed JNI libraries or legacy
packaging modes as a workaround for a library that fails the ELF check.

The focused JVM tests intentionally do not need native libraries:

```sh
gradle -p apps/android :app:testDebugUnitTest
```
