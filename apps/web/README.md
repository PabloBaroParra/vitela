# Vitela web shell (`apps/web`)

Batch 26. No server: the PDF engine is WASM running in a Web Worker, the UI
is TypeScript + React (Vite). The user's PDF never leaves the browser.

## Layout

```
src/
  main.tsx          boot only
  app/              engine worker start-up, engine status, shell layout, styles
  core/             typed worker client + shared types and message protocol
  worker/           the Web Worker: loads pdfium + core/pdf-wasm, serves requests
  features/
    document/       open a PDF
    render/         render a page and paint it into a canvas
  ui/               presentational pieces, no logic
e2e/                Playwright flows (production build)
scripts/            build-wasm.sh
generated/          build output of build-wasm.sh (git-ignored)
```

## The engine

The worker loads two WASM modules:

- **pdfium**: bblanchon `pdfium-binaries` `chromium/7763`, `pdfium-wasm.tgz`,
  downloaded and sha256-checked by `core/pdf-wasm/scripts/fetch-pdfium.sh`
  (which `scripts/build-wasm.sh` calls).
- **[`core/pdf-wasm`](../../core/pdf-wasm)**: the Rust core's wasm-bindgen
  boundary (B26 T-219) — open from bytes (no password, one, or both), page
  count and dimensions, render to RGBA at a DPI, text runs, page characters,
  search. Built from the root workspace with its `wasm-release` profile.
  `src/worker/engine.ts` is the only file that imports it.

Two pdfium-render workarounds live in `src/worker/pdfium.ts`, and both are
covered by tests:

- the `Module._PDFium_Init = Module._FPDF_InitLibrary` alias
- exposing the glue's `wasmExports`, where pdfium-render finds the function
  table when the glue is not loaded as a global script

`pdfium.real.test.ts` runs both against the real pinned pdfium.

## Build and test

Needs Node ≥ 22.12, `rustup target add wasm32-unknown-unknown`, and
`cargo install wasm-bindgen-cli --version 0.2.130 --locked`.

```bash
npm ci
```

```bash
npm run build:wasm    # core/pdf-wasm + pinned pdfium into generated/
```

```bash
npm test              # Vitest (includes the real-pdfium test, so build:wasm first)
```

```bash
npm run build         # type-check + production build into dist/
```

```bash
npx playwright install chromium firefox webkit
```

```bash
npm run e2e           # Playwright on the production build: Chromium, Firefox, WebKit
```

`npm run dev` serves the app with hot reload.

## Zero-network

`e2e/zero-network.spec.ts` is the web counterpart of `tools/zero-network-guard`.
After the app reports the engine ready, any request fails the test, including
requests from the worker. The test also checks that it can see the worker's own
`pdfium.wasm` fetch during load, so in every browser it is shown to be able to
see what it guards.
