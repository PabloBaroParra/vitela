# Vitela web shell (`apps/web`)

Batch 26. No server: the PDF engine is WASM running in a Web Worker, the UI
is TypeScript + React (Vite). The user's PDF never leaves the browser.

## Layout

```
src/
  main.tsx          boot only
  app/              engine worker start-up, engine status, shell layout, styles
  core/             typed worker client + shared types and message protocol
  worker/           the Web Worker: loads pdfium + the render module, serves requests
  features/
    document/       open a PDF
    render/         render a page and paint it into a canvas
  ui/               presentational pieces, no logic
e2e/                Playwright flows (production build)
render-wasm/        TEMPORARY Rust render crate (see below)
scripts/            build-wasm.sh
generated/          build output of build-wasm.sh (git-ignored)
```

## The engine today, and what replaces it

The worker loads two WASM modules:

- **pdfium**: bblanchon `pdfium-binaries` `chromium/7763`, `pdfium-wasm.tgz`,
  downloaded and sha256-checked by `scripts/build-wasm.sh`.
- **`render-wasm/`**: a small wasm-bindgen crate over `pdfium-render =0.9.5`
  that opens a document from bytes and renders a page to RGBA. It stands in
  for `core/pdf-wasm` (T-219). `src/worker/engine.ts` is the only file that
  imports it, so the swap stays inside `src/worker/`.

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
npm run build:wasm    # render-wasm + pinned pdfium into generated/
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
