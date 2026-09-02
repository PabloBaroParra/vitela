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

## icons/ — the shell's own icon set

`icons/*.svg` are the line icons the GTK4 shell draws in its app rail, its
Home tool grid and its quick actions. Hand-authored, like the brand mark, and
in the same 24x24 stroke style.

They exist because **no shell may look an icon up in the desktop's icon
theme**: the Linux build ships as an AppImage and a `.deb` (T-053) that has to
look the same on a host with a different theme, an incomplete one, or none,
and a lookup that misses leaves a blank control with no error anyone sees.
`apps/linux-gtk/src/app/shell.rs` states the same rule at the call site.

Unlike the brand mark — which is two authored files, one per theme — an icon
here is one file in one colour, recoloured as it is loaded. A tool's accent,
the muted grey of a disabled one and the neutral of a rail item are the same
drawing three times over, and three files per shape would be the same picture
maintained three times.

So each file carries the literal `#000000` **exactly once**, on the single
`<g>` that owns its strokes, and `apps/linux-gtk/src/app/icons.rs` substitutes
the colour it wants before handing the source to librsvg. Black rather than a
placeholder token so the file stays a valid, previewable SVG.
`icons::tests::every_icon_carries_exactly_one_tint_token` fails if a new icon
arrives with none (it would paint black everywhere) or with two (one stroke
would silently stay black), and
`gtk_ui_every_icon_rasterises_at_both_sizes` fails if a path is malformed —
neither of which is visible in a diff.

### The optical grid

**Every icon's strokes span y 3.5 to 20.5 of the 24-unit viewBox, centred on
12.** A new icon that does not is not finished.

This is not a style preference, it is the fix for a real defect. The first set
was authored shape by shape and each drawing filled its own box: rendered at
96px their ink ran from 60 to 80 pixels tall, and the folder and the signature
sat three pixels below centre. In the Home tool grid — where the *widget*
geometry is provably identical, every tile measuring 68px with a 24px icon and
a 16px label — the icons still visibly jumped up and down, and dragged the
apparent baseline of the labels under them along.

No widget assertion can catch that: to GTK a 24px `Picture` is a 24px
`Picture` whatever is drawn inside it. Only the pixels know, so
`icons::tests::gtk_ui_every_icon_is_drawn_on_the_same_optical_grid` rasterises
each icon and measures the first and last row with any ink in it, allowing one
pixel for antialiasing.

Widths may differ — a document is narrower than a four-square grid, and
forcing them equal would distort the drawings. Only the vertical band is
shared, because that is what the eye lines up on.

| Shell | How it is packaged | How it is drawn |
| ----- | ------------------ | --------------- |
| Linux (GTK4) | `include_str!` at compile time (`apps/linux-gtk/src/app/icons.rs`) | tinted, then rasterised through gdk-pixbuf's SVG loader into a `GdkTexture` |

No other shell uses these yet.
