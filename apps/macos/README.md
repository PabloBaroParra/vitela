# Vitela macOS development shell

This SwiftUI shell opens local PDFs through the byte-based `pdf-ffi` UniFFI
contract, presents ordered page slots in one vertical scroll view, and clamps
zoom to `0.5...4.0`. It is a development artifact only: it carries an ad-hoc
signature so it can run on Apple Silicon, but it is not notarized, installable,
or approved for distribution.

## Layout

Per the repository's "no monolithic shells" rule, the target is split by
responsibility:

- `Vitela/VitelaApp.swift` — entry point only, nothing else.
- `Vitela/PdfCoreClient.swift` — the only file that touches the generated
  UniFFI API, behind a protocol the rest of the app depends on.
- `Vitela/ViewerStore.swift` — viewer state: page slots, zoom, render requests.
- `Vitela/ViewerViewModel.swift` — file selection, the render queue, window title.
- `Vitela/Views/` — one file per view, plus the `RenderedPage → NSImage` bridge.

`ViewerStore` is main-thread only, with one documented exception:
`renderResult(for:)` runs on the render queue and touches nothing but the
immutable client and the request handed to it.

## Rendering and zoom

Each `PageSlot` records the zoom its current bitmap was rendered at. Changing
zoom makes visible slots stale, so they re-render at `72 * zoom` DPI instead of
upscaling the old bitmap; the previous bitmap stays on screen meanwhile. A slot
already current at the active zoom is never re-rendered when it scrolls back
into view, and a failed page waits for an explicit Retry rather than restarting
the same doomed work on every appearance.

## Building

`scripts/build-macos.sh --assemble` runs the whole chain: `cargo build
--release`, rewriting the cdylib's install name to `@rpath/libpdf_ffi.dylib`,
regenerating the Swift bindings, then `xcodebuild`. The Xcode target links
`-lpdf_ffi` out of `target/release` and finds `pdf_ffiFFI.h` through
`Generated/module.modulemap`; a build phase copies both dylibs into
`Contents/Frameworks`, so `xcodebuild test` also produces a test host that can
launch.

```sh
export PDFIUM_DYLIB=/absolute/path/to/libpdfium.dylib
bash scripts/build-macos.sh --assemble
bash scripts/build-macos.sh --verify-bundle build/Vitela.app
```

## macOS CI validation

The `macos.yml` workflow downloads the pinned PDFium 7763 universal build,
generates the Swift bindings (failing if `pdf_ffi.swift`, `pdf_ffiFFI.h`, or
`pdf_ffiFFI.modulemap` is missing), runs Rust and Swift tests, and assembles
`build/Vitela.app`. It then fails closed unless the bundle has an executable,
an `Info.plist`, both dylibs, and every bundled Mach-O slice declares
`minos <= 11.0`. No artifact uploads before that check succeeds.

`xcodebuild` is driven through the shared scheme at
`Vitela.xcodeproj/xcshareddata/xcschemes/Vitela.xcscheme`, so CI runs the same
build and test actions a local Xcode does.

`apps/macos/Generated/`, `apps/macos/DerivedData/`, and `build/Vitela.app/`
are generated outputs and can be removed to roll back the native shell without
affecting Rust core behavior or other platforms. A representative multi-page
PDF must still be opened, scrolled, and zoomed on physical Apple hardware
before any distribution decision.
