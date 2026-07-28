# Vitela

An offline-first, cross-platform PDF editor with a Rust core. View, annotate,
sign, and reorganize PDFs — without your documents ever leaving your machine.

*Vitela* is Spanish for vellum — the finest parchment, a writing support made
to last centuries. That is how this editor treats your documents: annotations
from other tools are preserved, encryption is never silently stripped, and
existing signatures survive your edits.

**Status: pre-release, under active development.** The core engine (rendering,
page operations, annotations, encrypted save, FFI surface) is complete and
tested; the platform shells (Linux, macOS, Windows, Android, iOS) are in
progress. There are no releases yet.

## Built with AI

Vitela is written with heavy AI assistance. Most of the code, the tests and this
documentation were produced by AI coding agents working from the architecture,
conventions and reviews of a human maintainer, who owns every change that lands.
The rules those agents follow are checked into the repository — see
[CLAUDE.md](CLAUDE.md) for the architectural constraints and
[AGENTS.md](AGENTS.md) for the workflow.

That is a statement about *how* the project is built, not an excuse for what it
does. Every change passes the same gates regardless of who or what wrote it:
`cargo fmt`, `clippy -D warnings`, the full workspace test suite, and the CI job
that runs the tests inside a network namespace with no routes. No capability is
marked `✅` in the tables below until its tests pass. If something is wrong, it
is an ordinary bug — please open an issue.

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
assets/          Product assets packaged into the shells — currently the
                 built-in sample document (see assets/README.md).
```

Key design decisions and per-batch acceptance criteria are documented in
[docs/batches-b8-b13.md](docs/batches-b8-b13.md).

## Tools & features

This is the full set of tools Vitela targets. The core engine (`pdf-manip`,
`pdf-save`, `pdf-annotate`, `pdf-render`) already implements the operations
marked **Core ready**; those become available in each platform shell as the
shells land. The rest are on the roadmap.

Legend: ✅ **Core ready** — engine implemented and tested · 🚧 **In progress /
next** — actively being built · 🔮 **Planned** — on the roadmap, not started.

| Tool | What it does | Crate | Status |
| ---- | ------------ | ----- | ------ |
| Merge PDF | Combine several PDFs into one | `pdf-manip` | ✅ Core ready |
| Split PDF | Split one PDF into multiple files | `pdf-manip` | ✅ Core ready |
| Organize pages | Reorder pages by drag-and-drop | `pdf-manip` | ✅ Core ready |
| Delete pages | Remove selected pages | `pdf-manip` | ✅ Core ready |
| Extract pages | Pull pages out into a new PDF | `pdf-manip` | ✅ Core ready |
| Rotate PDF | Rotate one or all pages | `pdf-manip` | ✅ Core ready |
| Protect PDF | Encrypt with a password (RC4-128 / AES-128) | `pdf-save` | ✅ Core ready |
| Unlock PDF | Remove a known password (decrypt-on-open) | `pdf-manip` | ✅ Core ready |
| PDF to images | Export pages as PNG / JPEG | `pdf-save` | ✅ Core ready |
| Sign PDF | Drawn signatures + PKCS#7/PAdES (offline, no TSA/OCSP) | `pdf-sign` | 🚧 In progress / next |
| Create PDF | Author a new PDF, including fillable forms | `pdf-form` *(planned)* | 🚧 In progress / next |
| Add watermark | Stamp text or image watermarks (alpha-aware) | `pdf-annotate` | 🚧 In progress / next |
| Add page numbers | Overlay page numbering | `pdf-annotate` | 🚧 In progress / next |
| PDF overlay | Stamp one PDF on top of another | `pdf-annotate` | 🚧 In progress / next |
| Images to PDF | Build a PDF from image files | — | 🔮 Planned |
| Extract images | Pull embedded images out of a PDF | — | 🔮 Planned |
| Compress PDF | Reduce file size | — | 🔮 Planned |
| Optimize for web | Linearize for fast web viewing | `pdf-save` | 🔮 Planned |
| Convert PDF | Convert to/from other document formats | — | 🔮 Planned |
| Compare PDF | Diff two PDFs side by side | — | 🔮 Planned |
| Edit PDF | Edit page body content (text / images) | — | 🔮 Planned |
| Redact PDF | Black out and remove sensitive content | — | 🔮 Planned |
| PDF OCR | Make scanned PDFs searchable | — | 🔮 Planned |

Note: rendering remote web pages to PDF is **deliberately excluded** — fetching
a URL would break the offline-first, zero-network guarantee.

### Fillable forms

Vitela treats forms as a first-class authoring feature, not just form-filling.
When you create a PDF, you can place **fillable fields** (text, checkbox,
choice, and more) directly on the page. Those fields are then surfaced as a
**form panel in a side rail**: fill an entry in the panel and the value is
written straight into the corresponding field on the PDF, live. The same panel
works for forms authored by other tools — Vitela reads existing AcroForm fields
and lets you fill them the same way. Output is standard AcroForm, so the filled
document renders correctly in Acrobat, Preview, and other spec-compliant
viewers (see [docs/batch-forms.md](docs/batch-forms.md)).

## Platform status

Where each native shell stands today. The core engine is shared; these rows
track what each shell has actually wired up and shipped, not what the engine
can do. Linux (GTK4) links the core crates directly; the others consume the
generated UniFFI bindings.

Legend: ✅ done & tested · 🚧 in progress · — not yet.

| Capability | Linux (GTK4) | Windows (WinUI 3) | macOS (SwiftUI) | Android (Compose) | iOS (SwiftUI) |
| ---------- | :----------: | :---------------: | :-------------: | :---------------: | :-----------: |
| Open a PDF | ✅ | ✅ | 🚧 | 🚧 | 🚧 |
| Built-in sample document | ✅ | ✅ | — | ✅ | — |
| Password-protected PDF | ✅ | — | — | 🚧 | — |
| Multi-page view & scroll | ✅ | ✅ | 🚧 | 🚧 | 🚧 |
| Fit-to-width rendering | ✅ | — | — | 🚧 | — |
| Text search & navigate | ✅ | ✅ | — | 🚧 | — |
| Print | ✅ | ✅ | — | 🚧 | — |
| Annotate | — | — | — | — | — |
| Save / export | — | — | — | — | — |
| Sign | — | — | — | — | — |
| Fillable forms | — | — | — | — | — |

Both Apple shells are `🚧` rather than `✅` for a specific reason. GitHub
Actions provides development-only evidence: for macOS it builds the shell, runs
its Swift tests and verifies the bundle; for iOS (still marked experimental) it
builds the shell, runs its unit tests on a simulator, and fails closed on the
iOS 15 floor the produced binaries actually declare. That is real automated
evidence, but it is not the same as a validated product — no test has yet run
on a physical Mac or iPhone, and signing, notarization, provisioning and public
distribution all remain unverified and deferred behind a paid Apple Developer
account. The two shells share their model layer (`apps/apple/Shared`); only the
app entry point, document picking and views are per-platform.

Android has a Compose baseline; its native package still requires externally
supplied PDFium Android libraries, so its capabilities remain in progress until
that runtime is validated on devices. Windows deliberately omits editing,
saving, and password UI in its first vertical.

> **Keeping this table honest (for humans and AI):** when a capability ships in
> a shell **and its tests pass**, flip its cell from `—` (or `🚧`) to `✅` in the
> same change. Never mark a cell `✅` before the feature is built and tested.
>
> CI backstops part of this: [`scripts/check_readme_tables.py`](scripts/check_readme_tables.py)
> (run in the `docs` workflow) fails the build if the *Tools & features* table
> names a crate that doesn't exist in `core/`, or if either table uses a status
> symbol outside its legend. It can't verify that a `✅` cell truly has passing
> tests — that part stays on you.

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

### The built-in sample document

Every shell packages `assets/sample/vitela-sample.pdf` and exposes it behind an
**Open sample** button, so a fresh install has something to render without the
user supplying a PDF first. All three shells package the same file; regenerate
it (byte-reproducibly) with:

```sh
cargo run -p gen-sample
```

See [assets/README.md](assets/README.md) for how each shell packages and reads
it.

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
PKCS#7/PAdES cryptographic signing (offline — no TSA/OCSP), and fillable
AcroForm forms — create, style, and fill standard form fields, including forms
authored by other tools (see [docs/batch-forms.md](docs/batch-forms.md)).
Later, post-MVP: page-body editing (text/images), OCR, and redaction. See
[Tools & features](#tools--features) for the full, per-tool status. Rendering
remote web pages to PDF stays out of scope — it would break the offline-first,
zero-network guarantee.

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
