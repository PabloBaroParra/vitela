# @vitela/core

Vitela's PDF engine for the browser: the Rust core (`core/pdf-wasm`) and
pdfium, both as WebAssembly, running in a module Web Worker behind a typed,
framework-agnostic client. No server: documents never leave the page.

Today the engine opens (with or without a password), renders pages to RGBA,
moves, rotates and removes pages (with undo/redo),
searches, selects text, and saves the document back to bytes.

## Install

```bash
npm install @vitela/core
```

## Usage

```ts
import { createEngine, PASSWORD_REQUIRED, errorKind } from '@vitela/core';

const engine = createEngine();
await engine.ready; // both WASM modules loaded

const bytes = new Uint8Array(await file.arrayBuffer());
try {
  // The buffer is transferred to the worker: `bytes` is detached afterwards.
  const { handle, info } = await engine.open(bytes);
  const page = await engine.renderPage(handle, 0, 96);
  canvas.width = page.width;
  canvas.height = page.height;
  canvas.getContext('2d')!.putImageData(new ImageData(page.rgba, page.width, page.height), 0, 0);
  await engine.close(handle);
} catch (error) {
  if (errorKind(error) === PASSWORD_REQUIRED) {
    // ask for a password and call engine.open(bytes, password)
  }
}
```

Every failure from the engine is an `EngineError` whose `name` is the core's
failure kind (`PasswordRequired`, `WrongPassword`, `TextExtractionDenied`,
`SignaturesWouldBeInvalidated`, `DocumentNotFound`, ...).

## API

- `createEngine(options?)` → `EngineClient`. The supported entry point: it
  spawns the worker, sends it the init message it waits for, and wraps it.
  It throws synchronously, before any worker exists, on an invalid option,
  on a relative URL with no page to resolve it against, or where there are
  no Web Workers (server-side rendering): call it in the browser.
  - `assetBaseUrl?: string | URL` — a directory serving `pdf_wasm_bg.wasm`,
    `pdfium.js` and `pdfium.wasm` (for example a CDN; it must allow CORS).
    Relative URLs resolve against the page. The files must be the ones this
    package version ships; see [CDN integrity](#cdn-integrity). Default: the
    copies shipped in `dist/generated/`, which your bundler emits as assets.
  - `workerUrl?: string | URL` — the worker script, for bundlers that do not
    follow `new Worker(new URL('…', import.meta.url))`. Build it from
    `dist/worker/worker.js`. Relative URLs resolve against the page.
  - `loadTimeoutMs?: number` — how long each of the three asset fetches may
    take before the engine fails (default `60000`).
- `EngineClient` — what `createEngine` returns; implements `Engine`: `ready`,
  `open`, `renderPage`, `close`, `saveToBytes`, `applyEdit`, `undo`, `redo`,
  `prepareImport`, `discardImport`, `importPrepared`, `search`, `selectText`,
  `selectWord`. An edit answers with the document as it is now
  (`EditedDocument`: page sizes, page ids, blocks, undo/redo state), and the
  engine has already refreshed its preview, so the next render shows it.
  Adding other PDFs takes two steps: `prepareImport(bytes, password?)` opens
  one source (a locked one rejects with `PasswordRequired`/`WrongPassword`
  until given its password), and `importPrepared(handle, ids, index)` adds
  every prepared source at once as one undo step (`ImportedDocument`: the
  edited document plus each source's block id); `discardImport` drops a
  source a pick abandoned.
  Its constructor is public, but wrapping a port yourself is
  not a supported setup: the worker answers nothing until it receives the
  init message that `createEngine` sends.
- `EngineError`, `errorKind`, `errorMessage`, and the kind constants
  `PASSWORD_REQUIRED`, `WRONG_PASSWORD`, `TEXT_EXTRACTION_DENIED`,
  `SIGNATURES_WOULD_BE_INVALIDATED`, `EDIT_NOT_PERMITTED`.
- Types: `Engine`, `DocumentHandle`, `DocumentInfo`, `OpenedDocument`,
  `PageEdit`, `EditedDocument`, `DocumentBlock`, `BlockSource`,
  `PreparedImport`, `ImportedDocument`,
  `PageSize`, `PagePoint`, `PageRects`, `RenderedPage`, `SearchHit`,
  `TextSelection`, and the protocol types `MessagePortLike`,
  `RequestEnvelope`, `WorkerMessage`.

All geometry is in PDF points on the page as drawn (top-left origin,
`/Rotate` applied); see the doc comments in `dist/core/types.d.ts`.

## Loading failures

`engine.ready` rejects (and every pending and later call fails) with a
message naming what went wrong: the URL and HTTP status of a file that did
not load, the URL of a fetch that hit a network or CORS error or ran past
`loadTimeoutMs`, or the file whose integrity check failed.

## CDN integrity

With `assetBaseUrl`, the three files come from a host this package does not
control, so the worker checks each one before compiling or evaluating it: it
hashes the bytes with WebCrypto (`crypto.subtle.digest('SHA-256', …)`) and
compares them with the sha256 of the exact files this package version ships
(computed when the package is built, `dist/generated/integrity.js`). Any
mismatch fails the engine with
`integrity check failed for <file>: sha256 is …, expected …`. So serve the
files from the same `@vitela/core` version, unmodified; a CDN that
modifies their bytes will be refused.

WebCrypto exists only in secure contexts, so a page using `assetBaseUrl`
must be served over HTTPS (or from `localhost`). The default, bundled copies
are part of your own build and are not checked.

## Content Security Policy

pdfium's Emscripten glue (`pdfium.js`) is not an ES module and cannot be
`importScripts`-ed in a module worker, so the worker evaluates its text with
`new Function`. If you set a CSP, it needs:

- `script-src 'unsafe-eval' 'wasm-unsafe-eval'` where the worker runs —
  `'unsafe-eval'` for that `new Function`, `'wasm-unsafe-eval'` to compile the
  two WebAssembly modules (`'unsafe-eval'` already implies it; list both);
- `worker-src` (on the page) allowing the worker script's origin (`'self'`
  for the bundled worker, or the origin of `workerUrl`);
- `connect-src` (where the worker runs) allowing wherever the three assets
  are fetched from (`'self'`, or the `assetBaseUrl` host).

Under CSP Level 3, a worker loaded from a URL is governed by the policy on
its own script's response, not the page's (only `blob:`/`data:` workers
inherit the page's). So if your server sends a CSP with every response,
the one on the worker script must carry the `script-src` and `connect-src`
above; the page's needs `worker-src`. For example, with bundled assets and
the same policy everywhere:

```
Content-Security-Policy: script-src 'self' 'unsafe-eval' 'wasm-unsafe-eval'; worker-src 'self'; connect-src 'self'
```

## Bundlers

The worker and the WASM files are referenced with
`new URL('…', import.meta.url)`, which Vite and webpack 5 follow out of the
box. Two settings matter:

- **Vite:** set `worker: { format: 'es' }` (the worker is a module worker),
  keep pdfium out of data: URIs with `build.assetsInlineLimit: 0`, and
  exclude the package from dependency pre-bundling, which would break the
  relative asset URLs: `optimizeDeps: { exclude: ['@vitela/core'] }`.
- **Other setups:** copy `dist/generated/engine/pdf_wasm_bg.wasm` and
  `dist/generated/pdfium/pdfium.{js,wasm}` to one directory and pass it as
  `assetBaseUrl` (they are then integrity-checked, see above).

## Building from source

In the Vitela repository (npm workspaces at the root):

```bash
npm ci
npm run build:wasm -w @vitela/core   # core/pdf-wasm + pinned pdfium into src/generated/
npm run build -w @vitela/core        # dist/: ESM + .d.ts, worker, WASM, digests
npm run watch -w @vitela/core        # keep dist/ current while editing
npm test -w @vitela/core             # Vitest, including the real-pdfium test
```

Builds overwrite `dist/` in place, so a dev server reading it keeps working
(`apps/web`'s `npm run dev` runs `watch` alongside Vite). `npm run clean`
removes it; `npm pack` / `npm publish` always clean first.

`build:wasm` needs `rustup target add wasm32-unknown-unknown` and
`cargo install wasm-bindgen-cli --version 0.2.130 --locked`.

## Layout

```
src/
  index.ts          public entry: re-exports only
  createEngine.ts   validates options, spawns the worker, sends it init, wraps it
  core/             typed worker client, shared types, message protocol, errors
  worker/           the Web Worker: plans, fetches (with a deadline) and verifies
                    the assets, loads pdfium + core/pdf-wasm, serves requests
  generated/        build output of scripts/build-wasm.sh and the asset digests
                    from scripts/write-integrity.mjs (git-ignored)
scripts/            build-wasm.sh, write-integrity.mjs, copy-assets.mjs,
                    watch.mjs, clean-dist.mjs
licenses/pdfium/    PDFium's licence and its third-party components' licences
```

## Licence

`MIT OR Apache-2.0`. The bundled pdfium binary keeps its own licence
(BSD-3-Clause, plus the licences of the libraries built into it); the full
texts ship in [`licenses/pdfium/`](licenses/pdfium) and are listed in
[NOTICE](NOTICE).
