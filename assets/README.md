# assets

Product assets shipped **inside** the platform shells — as opposed to
`tests/fixtures/`, which exists only to exercise the engine and never leaves
the repository.

## sample/ — the built-in sample document

`sample/vitela-sample.pdf` is a small, unencrypted, three-page PDF that every
shell packages, so a fresh install can render, scroll, search, and print
without the user first supplying a PDF of their own. Each shell exposes it
behind an **Open sample** button.

All three shells package this single file rather than keeping their own copy,
so a rendering difference between platforms is never down to different input:

| Shell | How it is packaged | How it is read |
| ----- | ------------------ | -------------- |
| Linux (GTK4) | `include_bytes!` at compile time (`apps/linux-gtk/src/app/document.rs`) | `PdfiumRenderer::open_document_from_bytes` |
| Windows (WinUI 3) | `None` item copied to `Assets\vitela-sample.pdf` beside the executable (`Pdf.Windows.csproj`) | `File.ReadAllBytesAsync` from `AppContext.BaseDirectory` |
| Android (Compose) | `assets.srcDir("../../../assets/sample")` (`app/build.gradle.kts`) | `AssetManager.open` via `SampleDocument` |

The document deliberately contains plain Helvetica text rather than a scanned
image: pdfium's text extraction backs the search feature, and the word
`vellum` appears on every page so "next/previous match" has something to step
through out of the box.

## Regenerating

The file is generated, not hand-authored. Output is byte-reproducible — a run
on an unchanged tree leaves `git status` clean:

```sh
cargo run -p gen-sample
```

The generator lives in [`tools/gen-sample/`](../tools/gen-sample). Its
integration test (`tests/committed_sample.rs`) fails if the committed file
drifts from the generator, or stops opening through `pdf-manip`, so a stale
sample cannot reach three platforms unnoticed.
