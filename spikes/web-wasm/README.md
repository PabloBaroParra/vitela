# web-wasm spike (Batch 26, phase 0)

Go/no-go spike for the Vitela web shell: does the Rust core run in a browser
as `wasm32-unknown-unknown`, and can pdfium render pages there with the
`pdfium_7763` pin, inside a Web Worker, with no server?

## Verdict: **GO**

Both halves ran in a real browser (Chromium 156, the Claude desktop browser
pane) inside one Web Worker:

- `pdf-manip` (lopdf) opened the PDF and counted its pages.
- pdfium (`chromium/7763`, Emscripten build) rendered page 1 at 1200 px wide
  to RGBA, and the main thread painted it into a `<canvas>`.

I checked the result on screen with screenshots and by counting pixels. The
fixtures were `forms/reportlab_acroform.pdf` (with its form value drawn),
`content-edit/reportlab_embedded_subset.pdf` and `compress/scan_200dpi.pdf`.
Two generated stress files also rendered: the last page of a 500-page PDF, and
page 20 of a 75 MB PDF holding 20 incompressible images.

You need two workarounds, and neither changes any `core/` crate (see "What it
took" below).

## Measured numbers

### Bundle (what the browser downloads; the fixture PDF is not counted)

| File | Raw | gzip -9 |
|---|---:|---:|
| `pkg/web_wasm_spike_bg.wasm` (pdf-manip + lopdf + pdfium-render) | 1,483,432 | 464,316 |
| `pkg/web_wasm_spike.js` (wasm-bindgen glue) | 27,604 | 6,080 |
| `pdfium/pdfium.wasm` (bblanchon `chromium/7763`) | 5,181,856 | 2,380,372 |
| `pdfium/pdfium.js` (Emscripten glue) | 162,494 | 29,807 |
| **Total** | **6,855,386 (6.5 MiB)** | **2,880,575 (2.7 MiB)** |

pdfium accounts for about 83% of the compressed size. The Rust module's size
depends heavily on the build profile:

| Rust build | Raw | gzip -9 |
|---|---:|---:|
| `opt-level=3`, keep name section | 4,506,313 | 645,360 |
| `opt-level=3`, `--remove-name-section` | 2,324,361 | 530,454 |
| **`opt-level=3` + `lto` + `codegen-units=1`** (used here) | **1,483,432** | **464,316** |
| `opt-level="z"` + `lto` + `codegen-units=1` | 1,213,705 | 415,932 |

`wasm-opt` was not installed and was not tried. The Rust build still includes
pdfium-render's `console_log` feature (debug tracing).

### Time to first render

Measured from navigation start to the moment the pixels were in the canvas.
The worker reports its own stages. Page: `reportlab_acroform.pdf` at
1200×1553.

| Run | pdfium ready | Rust ready | core open | pdfium render | **pixels on canvas** |
|---|---:|---:|---:|---:|---:|
| cold 1 | 50 ms | 58 ms | 13.4 ms | 43 ms | **192 ms** |
| cold 2 | 54 ms | 39 ms | 8.8 ms | 62 ms | **143 ms** |
| cold 3 | 44 ms | 38 ms | 8.9 ms | 63 ms | **132 ms** |
| warm 1 | 34 ms | 40 ms | 8.7 ms | 81 ms | **147 ms** |
| warm 2 | 30 ms | 36 ms | 9.4 ms | 64 ms | **128 ms** |
| 500-page PDF, last page | 34 ms* | 39 ms | 36 ms | 68 ms | 163 ms |
| 75 MB PDF, page 20 | 37 ms* | 42 ms | 40 ms | 166 ms | 398 ms |

The "ready" times are measured from worker start, and pdfium and the Rust
module load in parallel. The pdfium time is stamped in
`onRuntimeInitialized`. \*The two large-PDF rows came from an earlier build of
`worker.js` that stamped it only once `onmessage` awaited it, so for those rows
it is an upper bound. **Caveats, not verified:**

- Everything was served from `127.0.0.1`, so download time is ~0. "Cold" means
  the four module URLs (both `.js` and both `.wasm`) were cache-busted
  (`?cold`). In these runs the fixture PDF was not; `?cold` now busts it too,
  and the effect on the numbers is negligible for a 12 KB file. V8 may still
  have reused compiled wasm code.
- With the network included, the 2.7 MiB gzip bundle adds about 2.3 s at
  10 Mbit/s and about 0.45 s at 50 Mbit/s. This is arithmetic, not a
  measurement. No throttled run was done.
- The browser pane was hidden (a background page) during all runs, which can
  skew timing.
- Only one machine and one browser were used. Firefox, Safari and mobile were
  not tried.

## Exact versions / pins

| Thing | Version |
|---|---|
| rustc | 1.97.1, target `wasm32-unknown-unknown` |
| wasm-bindgen / wasm-bindgen-cli | 0.2.130 (`cargo install wasm-bindgen-cli --version 0.2.130 --locked`) |
| wasm-bindgen-test | 0.3.80 |
| pdfium-render | `=0.9.5` (the version the main `Cargo.lock` resolves today), features `pdfium_7763`, `thread_safe`, `console_log` |
| pdfium WASM | `bblanchon/pdfium-binaries` release `chromium/7763`, asset `pdfium-wasm.tgz`, sha256 `213cad62710dd7b9a6eff84fe35a4b7161aeba8aa41e4c9b46463c09a85601ca` (checked by `build.sh`; matches GitHub's asset digest) (`VERSION`: 148.0.7763.0; `args.gn`: no V8, no XFA, no partition_alloc) |
| lopdf | vendored `vendor/lopdf-0.45.0`, feature `wasm_js` |

`chromium/7763` is the same pdfium build the native shells pin, so the binding
surface matches the `pdfium_7763` feature exactly.
`paulocoutinhox/pdfium-lib`, which pdfium-render's docs recommend, has no
7763 release (its nearest are 7623 and 7902). It was therefore not used.

## What it took

### Step 1: core on wasm32

`pdf-document` + `pdf-manip` compiled for `wasm32-unknown-unknown` **with one
change: lopdf's `wasm_js` feature** (it turns on `getrandom/wasm_js`). It is
enabled from this crate's own `Cargo.toml`, so no `core/` file was touched.
Nothing else was needed:

- `rayon` and `chrono` (lopdf default features) both compile.
- At runtime, rayon falls back to the calling thread. lopdf runs `par_iter`
  over the xref on every load, so every test below exercises it.

`tests/core.rs` (5 tests, `wasm-bindgen-test` under Node) passes:

- opens a plain fixture
- refuses an AES-128 fixture without a password
- opens the AES-128 fixture with its user password
- opens an `/ObjStm` fixture (lopdf also decodes it through rayon's `par_chunks`)
- round-trips `save_to` → reopen in memory

### Step 2: pdfium in the browser

pdfium-render's WASM design binds at runtime to a **separate** Emscripten
module. Calls go through JS `ccall`, and bytes are copied between the two wasm
heaps. Two problems came up:

1. **Init-name mismatch (worked around in JS).** pdfium-render 0.9.5's WASM
   `FPDF_InitLibrary()` (`src/bindings/wasm_bindings.rs:1652-1668`) calls
   either `PDF_InitLibrary` or `PDFium_Init`, which are the names other WASM
   builds export. bblanchon's build exports only `_FPDF_InitLibrary`, so the
   first call panicked (`func is not a function`). Fix: one line in
   `www/worker.js`, `Module._PDFium_Init = Module._FPDF_InitLibrary`, before
   binding. After that, every other call used during open and render resolved.
   The rest of the ~480-function surface was **not** exercised: text,
   annotations, forms editing and save.
2. **The memory warning is stale.** pdfium-render's README says bblanchon's
   WASM builds have a non-growable heap and run out of memory after a few
   pages. The `chromium/7763` `pdfium.js` has a growable heap
   (`emscripten_resize_heap` → `wasmMemory.grow`, `getHeapMax() = 2 GiB`), and
   the 500-page and 75 MB runs above confirm it in practice.

A second, smaller trap: a background tab never runs `requestAnimationFrame`.
The page therefore measures as soon as `putImageData` returns.

### `thread_safe` and the `core/pdf-render` actor on wasm

- **`thread_safe` works as-is.** It only wraps each binding call in a `Mutex`.
  A wasm32 module runs on one thread, so the lock is always free. The browser
  render went through `ThreadSafePdfiumBindings`, and
  `tests/platform.rs::uncontended_mutex_works` pins this. You can keep the
  feature or drop it on wasm. It costs nothing either way.
- **The actor does not work.** Each blocking primitive it uses fails on
  wasm32. The first two are pinned by `tests/platform.rs`; the others were
  measured with throwaway probes:
  - `std::thread::Builder::spawn` returns `Err`. `Actor::spawn` would panic on
    its `.expect`.
  - `Condvar::wait`, which the worker loop parks on, panics with "condvar wait
    not supported".
  - `mpsc::Receiver::recv()` on an empty channel, which is `JobHandle::wait()`
    (`actor.rs:286`), **never returns**. The probe was killed by `timeout`
    after 90 s. This is worse than a panic: a caller waiting on a job freezes
    the worker with no error.
  - The timed variants panic too: `Condvar::wait_timeout` ("can't sleep") and
    `recv_timeout` ("time not implemented on this platform").
  - Non-blocking `try_recv` works, which `tests/platform.rs` also pins.
- **`core/pdf-render` does not compile for wasm32 today.** There are 4
  errors, all from loading pdfium as a native library:
  - `Pdfium::bind_to_library` (`renderer.rs:52`)
  - `Pdfium::pdfium_platform_library_name` (`library.rs:32,37`)
  - `load_pdf_from_file` (`renderer.rs:99`)

  None of these exist on wasm in pdfium-render.

## What `core/pdf-render` would need to change

1. **Split binding from rendering.** On wasm, binding is
   `Pdfium::bind_to_system_library()` after JS has called
   `initialize_pdfium_render(pdfiumModule, rustModule, debug)` (plus the alias
   above). Put `library.rs` and the path-based `bind_actor` behind
   `#[cfg(not(target_arch = "wasm32"))]`.
2. **Make the actor optional.** The render work (`PdfiumState`, the document
   registry, bitmap and inversion code) should be callable **synchronously**
   on the current thread. Native keeps `Actor::spawn` around it. On the web,
   **the Web Worker is the actor**: it already owns pdfium on one thread, and
   the priority queue and cancel-at-dequeue move to the worker's message
   loop (in Rust or in JS), between `postMessage`s. Nothing may call
   `JobHandle::wait()` on wasm.
3. **Open from bytes only.** `load_pdf_from_byte_slice` / `_vec` instead of
   `load_pdf_from_file` (the web has no paths). The bytes are copied into
   pdfium's heap, so a large PDF sits in memory twice (Rust heap and pdfium
   heap). Budget for it.
4. Return pixels as owned RGBA `Vec<u8>` (or
   `PdfBitmap::as_array()` → `Uint8Array`) and transfer the `ArrayBuffer` to
   the main thread, as this spike does.

## Open risks for B26 (not tested here)

- **The spike's own `Cargo.lock` is not audited.** cargo-audit, cargo-deny and
  Dependabot run against the root workspace only. That is acceptable for a
  spike. The real `core/pdf-wasm` should join the root workspace so the
  existing gates cover it.

- **Fonts.** The Emscripten pdfium has no system fonts. Embedded fonts and the
  base-14 fonts rendered. Non-embedded, non-base-14 fonts (CJK above all) were
  not tried and will fall back to substitutes.
- **Per-call overhead.** Every `FPDF_*` call crosses JS (`ccall`). That is
  fine for one render (~40-80 ms per page at 1200 px), but text extraction or
  hit-testing that makes thousands of calls per page has not been measured.
- **Upstream fragility.** The init alias depends on pdfium-render's lookup
  order. Fix it upstream, or pin the version and test the alias.
- **pdfium's 2 GiB heap ceiling**, and memory overall: two heaps plus RGBA
  buffers. Not measured.

## Fallback (pdf.js for painting only)

**Not evaluated, because the primary path is GO.** If it were ever needed, the
cost would be:

- a second PDF engine whose rendering differs from what the native shells
  show
- overlay geometry and hit-testing computed against pdf.js's model instead of
  pdfium's

pdf.js's bundle size was not measured.

## Reproduce

Requires `rustup target add wasm32-unknown-unknown`, wasm-bindgen-cli 0.2.130,
`gh` (to download pdfium), `sha256sum`, `gzip` and Python. Run from `spikes/web-wasm/`:

```bash
cargo test --target wasm32-unknown-unknown   # Node runner, via .cargo/config.toml
```

```bash
bash build.sh                                 # builds www/, downloads pdfium, prints sizes
```

```bash
python -m http.server 8765 --bind 127.0.0.1 --directory www
```

Then open `http://127.0.0.1:8765/index.html`. Query parameters:

- `f=<fixture>`: any file in `www/fixtures/`
- `p=<page index>`
- `cold`: cache-bust every asset
- `debug`: pdfium-render trace logging in the console

The page prints its timing JSON and exposes it as `window.spikeResult`.

The two stress PDFs were generated with reportlab and are not committed:

- a 500-page file: 40 lines of Times text per page
- a 20-page file: each page holds a 1000×1000 `os.urandom` RGB image
