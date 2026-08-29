# assets

Product assets shipped **inside** the platform shells — as opposed to
`tests/fixtures/`, which exists only to exercise the engine and never leaves
the repository.

## sample/ — the built-in sample document

`sample/vitela-sample.pdf` is a small, unencrypted, three-page PDF that every
shell packages, so a fresh install can render, scroll, search, and print
without the user first supplying a PDF of their own. Each shell exposes it
behind an **Open sample** button.

All shells package this single file rather than keeping their own copy, so a
rendering difference between platforms is never down to different input:

| Shell | How it is packaged | How it is read |
| ----- | ------------------ | -------------- |
| Linux (GTK4) | `include_bytes!` at compile time (`apps/linux-gtk/src/app/document.rs`) | `PdfiumRenderer::open_document_from_bytes` |
| Windows (WinUI 3) | `None` item copied to `Assets\vitela-sample.pdf` beside the executable (`Pdf.Windows.csproj`) | `File.ReadAllBytesAsync` from `AppContext.BaseDirectory` |
| macOS (SwiftUI) | Resources build phase copies it into `Contents/Resources/vitela-sample.pdf` (`Vitela.xcodeproj`) | `Bundle.main.url(forResource:withExtension:)` in `ViewerViewModel.loadBundledSample` |
| iOS (SwiftUI) | Resources build phase copies it into the app bundle (`VitelaIOS.xcodeproj`) | `Bundle.main.url(forResource:withExtension:)` in `ViewerViewModel.loadBundledSample` |
| Android (Compose) | `assets.srcDir("../../../assets/sample")` (`app/build.gradle.kts`) | `AssetManager.open` via `SampleDocument` |

### Encrypted samples

`sample/aes_128_user_and_owner.pdf` and `sample/rc4_128_user_and_owner.pdf` are
copies of the corresponding fixtures from `tests/fixtures/encrypted/` (see
`tests/fixtures/README.md`), packaged the same way as the plain sample so
every shell's **Open sample** control can also open a password-protected file
without the user first supplying one. User passwords: `user-aes-pass` /
`user-rc4-pass`.

| Shell | How it is packaged | How it is read |
| ----- | ------------------ | -------------- |
| Linux (GTK4) | `include_bytes!` at compile time (`apps/linux-gtk/src/app/document.rs`) | `PdfiumRenderer::open_document_from_bytes` |
| Windows (WinUI 3) | `None` items copied to `Assets\` beside the executable (`Pdf.Windows.csproj`) | `File.ReadAllBytesAsync` from `AppContext.BaseDirectory` |
| macOS (SwiftUI) | Resources build phase copies them into `Contents/Resources/` (`Vitela.xcodeproj`) | `Bundle.main.url(forResource:withExtension:)` in `ViewerViewModel.loadBundledResource(named:)` |
| iOS (SwiftUI) | Resources build phase copies them into the app bundle (`VitelaIOS.xcodeproj`) | `Bundle.main.url(forResource:withExtension:)` in `ViewerViewModel.loadBundledResource(named:)` |
| Android (Compose) | `assets.srcDir("../../../assets/sample")` (`app/build.gradle.kts`) | `AssetManager.open` via `SampleDocument` |

The document deliberately contains plain Helvetica text rather than a scanned
image: pdfium's text extraction backs the search feature, and the word
`vellum` appears on every page so "next/previous match" has something to step
through out of the box.

### Regenerating

The file is generated, not hand-authored. Output is byte-reproducible — a run
on an unchanged tree leaves `git status` clean:

```sh
cargo run -p gen-sample
```

The generator lives in [`tools/gen-sample/`](../tools/gen-sample). Its
integration test (`tests/committed_sample.rs`) fails if the committed file
drifts from the generator, or stops opening through `pdf-manip`, so a stale
sample cannot reach three platforms unnoticed.

## brand/ — the application mark

`brand/vitela-app-mark.svg` and `brand/vitela-app-mark-dark.svg` are the
isotype a shell shows in the page area while no document is open. Two files
rather than one tinted at runtime: the navy reads on a light theme and
vanishes on a dark one, and the paths carry a literal fill rather than
`currentColor`, which neither shell's SVG pipeline would resolve anyway.

Unlike the sample, these are hand-authored — there is no generator to re-run.

| Shell | How it is packaged | How it is drawn |
| ----- | ------------------ | --------------- |
| Linux (GTK4) | `include_bytes!` at compile time (`apps/linux-gtk/src/app/brand.rs`) | rasterised through gdk-pixbuf's SVG loader into a `GdkTexture`, overlaid on the scroller |
| Windows (WinUI 3) | `None` items copied to `Assets\` beside the executable (`Pdf.Windows.csproj`) | `SvgImageSource` picked per theme in `App.xaml` |

The Android shell does not show the mark yet.
