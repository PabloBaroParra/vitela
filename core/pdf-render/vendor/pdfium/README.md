# vendor/pdfium

This directory is a **local, gitignored** cache of the prebuilt pdfium
dynamic library used by `pdf-render`'s tests (and, optionally, local dev
runs). See `design.md`'s "PDFium Binary Distribution" section — production
shells bundle their own platform copy and point at it via the
`PDFIUM_DYNAMIC_LIB_PATH` environment variable (see `../../src/library.rs`);
this `vendor/` copy exists purely so `cargo test -p pdf-render` works out of
the box on a fresh checkout without requiring that env var.

Nothing under `vendor/pdfium/` other than this README is committed (see the
root `.gitignore`).

## Populating it

Download the matching release from
[bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries) —
pin to the same Chromium/pdfium build that `pdfium-render`'s enabled
`pdfium_XXXX` Cargo feature targets (currently `pdfium_7763`, see
`../../Cargo.toml`). Use the **non-V8** build (no XFA/JavaScript engine —
this project doesn't need it, and it's a much smaller download).

### Windows (x64)

```sh
curl -sL "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/7763/pdfium-win-x64.tgz" -o pdfium-win-x64.tgz
tar -xzf pdfium-win-x64.tgz -C .
rm pdfium-win-x64.tgz
```

This produces `bin/pdfium.dll` (plus `include/`, `LICENSE.md`, etc.), which
`../../src/library.rs`'s `resolve_library_path()` looks for automatically.

### Linux (x86_64) / macOS (universal)

Same idea, swap the asset name for `pdfium-linux-x64.tgz` /
`pdfium-mac-univ.tgz` from the same release tag; the resulting
`bin/libpdfium.so` / `lib/libpdfium.dylib` is found the same way (see
`Pdfium::pdfium_platform_library_name()` in the `pdfium-render` crate for the
exact per-platform file name pdfium-render expects).

## License

pdfium-binaries ships pdfium under its own (BSD/Apache-derivative) license —
see the downloaded `LICENSE.md`. Compatible with this project's `MIT OR
Apache-2.0` license; verify per-release per `design.md`'s note.
