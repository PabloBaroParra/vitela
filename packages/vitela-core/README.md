# @vitela/core

Vitela's PDF engine for the browser: the Rust core (`core/pdf-wasm`) and
pdfium, both as WebAssembly, running in a module Web Worker behind a typed,
framework-agnostic client. No server: documents never leave the page.

Today the engine opens (with or without a password), renders pages to RGBA,
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

- `createEngine(options?)` → `EngineClient`. Spawns the worker.
  - `assetBaseUrl?: string | URL` — a directory serving `pdf_wasm_bg.wasm`,
    `pdfium.js` and `pdfium.wasm` (for example a CDN; it must allow CORS).
    Relative URLs resolve against the page. Default: the copies shipped in
    `dist/generated/`, which your bundler emits as assets.
  - `workerUrl?: string | URL` — the worker script, for bundlers that do not
    follow `new Worker(new URL('…', import.meta.url))`. Build it from
    `dist/worker/worker.js`.
- `EngineClient` — implements `Engine`: `ready`, `open`, `renderPage`,
  `close`, `saveToBytes`, `search`, `selectText`, `selectWord`. It can also
  wrap any `MessagePortLike` that speaks the worker protocol.
- `EngineError`, `errorKind`, `errorMessage`, and the kind constants
  `PASSWORD_REQUIRED`, `WRONG_PASSWORD`, `TEXT_EXTRACTION_DENIED`,
  `SIGNATURES_WOULD_BE_INVALIDATED`.
- Types: `Engine`, `DocumentHandle`, `DocumentInfo`, `OpenedDocument`,
  `PageSize`, `PagePoint`, `PageRects`, `RenderedPage`, `SearchHit`,
  `TextSelection`, and the protocol types `MessagePortLike`,
  `RequestEnvelope`, `WorkerMessage`.

All geometry is in PDF points on the page as drawn (top-left origin,
`/Rotate` applied); see the doc comments in `dist/core/types.d.ts`.

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
  `assetBaseUrl`.

## Building from source

In the Vitela repository (npm workspaces at the root):

```bash
npm ci
npm run build:wasm -w @vitela/core   # core/pdf-wasm + pinned pdfium into src/generated/
npm run build -w @vitela/core        # dist/: ESM + .d.ts, worker and WASM
npm test -w @vitela/core             # Vitest, including the real-pdfium test
```

`build:wasm` needs `rustup target add wasm32-unknown-unknown` and
`cargo install wasm-bindgen-cli --version 0.2.130 --locked`.

## Layout

```
src/
  index.ts          public entry: re-exports only
  createEngine.ts   spawns the worker, sends it the asset base, wraps it
  core/             typed worker client, shared types, message protocol, errors
  worker/           the Web Worker: loads pdfium + core/pdf-wasm, serves requests
  generated/        build output of scripts/build-wasm.sh (git-ignored)
scripts/            build-wasm.sh, and the dist/ clean + asset copy steps
```

## Licence

`MIT OR Apache-2.0`. The bundled pdfium binary keeps its own licence; see
[NOTICE](NOTICE).
