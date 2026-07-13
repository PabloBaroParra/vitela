# PDF Editor (working title)

An offline-first, cross-platform PDF editor with a Rust core. View, annotate,
sign, and reorganize PDFs — without your documents ever leaving your machine.

**Status: pre-release, under active development.** The core engine (rendering,
page operations, annotations, encrypted save, FFI surface) is complete and
tested; the platform shells (Linux, macOS, Windows, Android, iOS) are in
progress. There are no releases yet.

## Principles

- **Offline-first, zero telemetry.** No network calls, ever. CI enforces this
  by running the test suite inside a network namespace with no routes.
- **Your documents stay intact.** Annotations from other editors are preserved,
  encryption is re-applied on save by default (never silently stripped), and
  saves are incremental where possible — so existing digital signatures are
  not invalidated by editing.
- **Standards-compliant output.** Annotations are written as standard PDF
  objects that render correctly in Acrobat, Preview, and other spec-compliant
  viewers.
- **One core, native shells.** All document logic lives in Rust crates; each
  platform gets a thin native UI (GTK4, SwiftUI, WinUI3, Jetpack Compose) over
  the same UniFFI bindings.

## Architecture

```
core/
  pdf-document   Pure document model: pages, annotations, undoable EditLog,
                 non-undoable audit log, security context. No I/O.
  pdf-render     Rendering via pdfium, serialized through a single-threaded
                 actor with priority queueing and cancellation.
  pdf-manip      lopdf-backed document manipulation: merge, split, page ops,
                 decrypt-on-open (RC4-128 / AES-128, user & owner passwords).
  pdf-annotate   Builders for the standard annotation types, including image
                 stamps with alpha (/AP appearance streams with SMask).
  pdf-save       Incremental and full-rewrite writers, encryption
                 preservation, deterministic output hooks for CI, PNG/JPEG
                 export.
  pdf-ffi        The UniFFI boundary: document/bitmap handles, typed errors,
                 bytes-based open/save as the canonical cross-platform
                 contract. The only crate that depends on uniffi.
apps/            Platform shells (in progress). The Linux GTK4 shell consumes
                 the core crates directly; macOS/Windows/Android/iOS consume
                 generated Swift/C#/Kotlin bindings.
```

Key design decisions and per-batch acceptance criteria are documented in
[docs/batches-b8-b13.md](docs/batches-b8-b13.md).

## Building

Requires stable Rust (edition 2021).

```sh
cargo build --workspace
```

### Running tests

`pdf-render`'s tests need the prebuilt pdfium dynamic library, which is not
committed. Populate the local cache once (one `curl` + `tar`, ~5MB) following
[core/pdf-render/vendor/pdfium/README.md](core/pdf-render/vendor/pdfium/README.md),
then:

```sh
cargo test --workspace
```

Everything else — encrypted-PDF fixtures included — is generated or committed
and works on a fresh checkout.

### Generating FFI bindings

```sh
cargo run -p pdf-ffi --features bindgen --bin uniffi-bindgen -- \
    generate --library target/debug/libpdf_ffi.so \
    --language swift --out-dir target/bindings/swift
```

(Swap the library extension per platform: `.dll` / `.dylib` / `.so`. C#
bindings use the external `uniffi-bindgen-cs` tool — see
[spikes/uniffi-cs/README.md](spikes/uniffi-cs/README.md) for the exact version
pinning, which matters.)

## Roadmap

Rendering, page operations, annotations, encrypted save, and the FFI surface
are done. In progress / upcoming: the five platform shells, drawn signatures,
and PKCS#7/PAdES cryptographic signing (offline — no TSA/OCSP). Deliberately
out of scope for the MVP: text/image body editing, forms, OCR, redaction.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

pdfium is used as a prebuilt binary from
[bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)
under its own BSD/Apache-derivative license.
