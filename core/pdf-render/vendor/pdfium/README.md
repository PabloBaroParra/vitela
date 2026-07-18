# vendor/pdfium

This directory is a **local, gitignored** cache of the prebuilt pdfium
dynamic library used by `pdf-render`'s tests (and, optionally, local dev
runs). See `design.md`'s "PDFium Binary Distribution" section — production
shells bundle their own platform copy and point at it via the
`PDFIUM_DYNAMIC_LIB_PATH` environment variable (see `../../src/library.rs`);
this `vendor/` copy exists purely so `cargo test -p pdf-render` works out of
the box on a fresh checkout without requiring that env var.

Note that the zero-config lookup only pans out on Windows: `resolve_library_path()`
probes `vendor/pdfium/bin/`, and only the Windows tarball ships the library
there. On Linux/macOS the library lands in `lib/`, so those platforms need the
env var (or a move into `bin/`) — see "Populating it" below.

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
`pdfium-mac-univ.tgz` from the same release tag:

```sh
mkdir -p core/pdf-render/vendor/pdfium
curl -sL "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/7763/pdfium-linux-x64.tgz" \
  | tar -xz -C core/pdf-render/vendor/pdfium
```

**Unlike the Windows asset, these tarballs ship the library under `lib/`, not
`bin/`.** `resolve_library_path()` only probes `vendor/pdfium/bin/`, so the
zero-config vendored-dir lookup does *not* find it. Pick one:

Point the env override at the extracted file — what CI does, see
`.github/workflows/core.yml`:

```sh
export PDFIUM_DYNAMIC_LIB_PATH="$PWD/core/pdf-render/vendor/pdfium/lib/libpdfium.so"
```

Or move the library into `bin/` to restore the zero-config path:

```sh
mkdir -p core/pdf-render/vendor/pdfium/bin
mv core/pdf-render/vendor/pdfium/lib/libpdfium.so core/pdf-render/vendor/pdfium/bin/
```

See `Pdfium::pdfium_platform_library_name()` in the `pdfium-render` crate for
the exact per-platform file name pdfium-render expects.

## License

pdfium-binaries ships pdfium under its own (BSD/Apache-derivative) license —
see the downloaded `LICENSE.md`. Compatible with this project's `MIT OR
Apache-2.0` license; verify per-release per `design.md`'s note.
