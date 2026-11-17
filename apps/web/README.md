# Vitela web shell (`apps/web`)

Batch 26. No server: the PDF engine is WASM running in a Web Worker, the UI
is TypeScript + React (Vite). The user's PDF never leaves the browser.

## Layout

```
src/
  main.tsx          boot only: starts the engine (createEngine) and renders App
  app/              engine status, shell layout, styles
  features/
    document/       open a PDF: picker, drop on the window, password dialog
    viewer/         virtualized page column, zoom (pure layout/zoom math + hooks)
    render/         render a page and paint it into a canvas
    search/         find in the document: match navigation (pure) + hook
    text/           text layer, selection, copy, the text toolbar
  ui/               presentational pieces, no logic
  test/             shared test fakes (Vitest only)
e2e/                Playwright flows (production build); pdf.ts builds test PDFs
```

## The engine

The engine lives in the
[`@vitela/core`](../../packages/vitela-core) package (npm workspace): the
typed worker client, the Web Worker, and the WASM build. The app imports only
its public entry (`@vitela/core`), and consumes its built `dist/` exactly as
an npm dependent would, so every script below builds the package first
(`npm run build:core`).

The worker loads two WASM modules:

- **pdfium**: bblanchon `pdfium-binaries` `chromium/7763`, `pdfium-wasm.tgz`,
  downloaded and sha256-checked by `core/pdf-wasm/scripts/fetch-pdfium.sh`
  (which the package's `scripts/build-wasm.sh` calls).
- **[`core/pdf-wasm`](../../core/pdf-wasm)**: the Rust core's wasm-bindgen
  boundary (B26 T-219) — open from bytes (no password, one, or both), page
  count and dimensions, render to RGBA at a DPI, text runs, page characters,
  search, and the placement of text rects on the page as drawn
  (`placeRects` / `pointToPdf`). Built from the root workspace with its
  `wasm-release` profile.
  The package's `src/worker/engine.ts` is the only file that imports it.

Two pdfium-render workarounds live in the package's `src/worker/pdfium.ts`,
and both are covered by tests:

- the `Module._PDFium_Init = Module._FPDF_InitLibrary` alias
- exposing the glue's `wasmExports`, where pdfium-render finds the function
  table when the glue is not loaded as a global script

`pdfium.real.test.ts` (in the package) runs both against the real pinned
pdfium.

## Build and test

Needs Node ≥ 22.12, `rustup target add wasm32-unknown-unknown`, and
`cargo install wasm-bindgen-cli --version 0.2.130 --locked`. Dependencies
install at the repository root (one lockfile for the web workspaces); the
other commands run in `apps/web`.

```bash
npm ci                # at the repository root
```

```bash
npm run build:wasm    # core/pdf-wasm + pinned pdfium into packages/vitela-core/src/generated/
```

```bash
npm test              # builds @vitela/core, then the app's Vitest
npm test -w @vitela/core   # the engine's own Vitest (includes the real-pdfium test)
```

```bash
npm run build         # builds @vitela/core, type-checks, production build into dist/
```

```bash
npx playwright install chromium firefox webkit
```

```bash
npm run e2e           # Playwright on the production build: Chromium, Firefox, WebKit
```

`npm run dev` serves the app with hot reload, and runs `@vitela/core`'s
watch build (`scripts/dev.mjs`) alongside Vite, so edits inside
`packages/vitela-core/src` reach the running app without a restart. After
`npm run build:wasm`, the watcher re-copies the new WASM into the package's
`dist/` too.

## The viewer

- **Open:** the "Open PDF" picker, or drop a file anywhere on the window. A new
  document replaces the open one only once it opened; a failed or cancelled
  open leaves the current one in place.
- **Passwords:** the worker forwards the core's error `name`. `PasswordRequired`
  and `WrongPassword` bring up a modal `<dialog>` (focus stays inside it,
  Escape cancels); any other failure is shown as "Could not open …".
- **Scrolling:** every page is laid out from the core's `pageDimensions`
  (already turned by `/Rotate`), and only the pages within one viewport of the
  visible area are mounted and rendered. Far-away pages hold no canvas.
- **Zoom:** 25%–400%. 100% is physical size (96 CSS px per inch). +/- step
  through presets, "Fit width" fits the widest page, and Ctrl/⌘+wheel zooms
  around the pointer (a plain wheel scrolls). Pages re-render at
  `round(96 × zoom × devicePixelRatio)` dpi, capped at 12 Mpx per page, 150 ms
  after the zoom stops changing; until then the previous bitmap is scaled.

## Search and text selection

- **Text permission:** `open` reports `textExtractionAllowed` (the core's
  rule: PDF permission bit 5, or the owner password). When it is `false` the
  search box and Copy are disabled, pages take no text input, and a note
  says why. Every text read also goes through the core's gate: an engine
  refusal named `TextExtractionDenied` turns the same features off.
- **Geometry:** the worker hands back rects on the page *as drawn* — top-left
  origin, `/Rotate` applied — through the core's own `place_rect` /
  `point_to_pdf` (exported by `core/pdf-wasm` as `placeRects` /
  `pointToPdf`). The UI only multiplies by `CSS_PX_PER_PT × zoom`. Highlights
  and the selection live in each page's `TextLayer`, inside its `PageSlot`.
- **Search:** the core's search, exact and case-sensitive; nothing is matched
  in TypeScript. Enter searches (Enter again or ↓ moves on, Shift+Enter or ↑
  goes back, wrapping). Each match is one rect per line (`SearchMatch.lines`).
  Moving to a match scrolls it into view from the layout, so a page that is
  not mounted yet works the same.
- **Selection:** drag on a page (or double-click a word). The selection stays
  on the page the press landed on. The worker keeps the characters of the
  four most recently used pages; one request is in flight at a time and the
  moves made meanwhile collapse into the latest.
- **Copy:** Copy or Ctrl/⌘+C writes the core's text for the selection
  (`textIn`) with `navigator.clipboard.writeText`. Ctrl/⌘+C in a field, or
  over text the browser itself selected, is left to the browser.

E2E fixtures come from `tests/fixtures/` (the encrypted one's passwords are in
its README). Documents the corpus lacks — 60 pages, a `/Rotate 90` page, text
at known positions, an RC4-encrypted PDF that forbids copying — are built in
memory by `e2e/pdf.ts` and never written to disk.

## Saving

`features/save/`. **Save** asks the engine for the document's bytes
(`saveToBytes`, transferred back from the worker, never copied) and delivers them
one of three ways:

- **Over the opened file** when the document came in with a `FileSystemFileHandle`:
  Chromium's `showOpenFilePicker` (the Open button uses it when the browser has
  it; the `<input type=file>` stays as the fallback and the keyboard target) or a
  drop's `getAsFileSystemHandle` (kept only when its name matches the dropped
  file). Write permission is requested as the **first** step of the Save click,
  before the engine runs — Chromium's user activation can expire while the
  worker saves — and the answer travels with the bytes (`delivery.ts`:
  `prepare`, then `deliver`). If it is refused the document is downloaded and
  the status line says the permission was the reason. Chromium writes to a swap
  file and replaces the original on `close()`; a failed `write()`/`close()`
  calls `abort()` first, so a failed write leaves the user's file as it was.
  **If the file changed on disk since it was opened** (its `lastModified` or
  `size` differs from what `platform/fileStamp.ts` recorded at open), nothing is
  written: a dialog (`ChangedOnDiskDialog`, Cancel focused) offers **Cancel**,
  **Save a copy** (a download; the file is not touched) and **Overwrite**. The
  check is part of `prepare`, after the write-permission prompt and before the
  engine, and is repeated right before the write: if the file moved while the
  engine worked, nothing is written and the dialog shows again. Choosing
  Overwrite only counts once the write succeeded. If the file cannot be read
  (`getFile` fails) the document is downloaded and the status says why; the next
  save tries again. On a signed document, a copy's signature question offers only
  Cancel and "Save copy anyway". After a successful
  write the recorded state is refreshed, so the next save does not false-alarm.
- **As a copy** (`saveCopy.ts`), only from the signature question: always a
  download named `name (copy).pdf`. It deliberately does not use
  `showSaveFilePicker`: Chromium truncates an existing file picked there before
  returning its handle, so picking the opened file would empty it. The opened
  file's handle is never touched.
- **As a download** (anchor + Blob URL) everywhere else: Firefox, WebKit, a file
  opened through the plain input, or a handle that refuses the write — in which
  case the status line says it downloaded *instead*.

A save that would break a signature is refused by the core
(`SignaturesWouldBeInvalidated`); the app asks first (`SignatureDialog`, same
words as the Linux shell) with **Cancel** (focused), **Save a copy** and **Save
anyway**, and retries with the acknowledgement. The copy carries the broken
signature; the original, untouched, keeps its valid one. An encrypted document
keeps its encryption (the core's default save intent).

The save state (`useSave`) lives in `DocumentWorkspace`, one per open document.
It holds a `dirty` flag: editing screens (Organize, T-226) call `useMarkDirty()`
after each applied edit. Only a save that **wrote over the opened file** clears
it, and only if no edit came in while it ran; a download or a copy leaves the
opened file without the edits, so the status says it still has unsaved changes.
An edit after a save turns "Saved over…" into "Unsaved changes."; nothing sets
it yet. A hung engine worker is not handled (it would need a worker restart and
a reopen). The Save
button uses `aria-disabled` while it runs, so the keyboard focus stays on it.

The file-handle adapter (`fileHandle`, `fileWrite`, `saveTarget`) lives in
`src/platform/`, shared by `ui/`, `features/document/` and `features/save/`.

E2E (`e2e/save.spec.ts`): the download flow in all three browsers, reopening the
saved bytes, and the keyboard focus staying on Save. In Chromium the write-over flows use real handles from the
origin-private file system (`navigator.storage.getDirectory()`) fed in through
`showOpenFilePicker` and `getAsFileSystemHandle`, because Playwright cannot drive
the native picker (a refused permission, a mismatched drop handle and a failing
picker are covered the same way). **Not covered:** the browser's own permission
prompt, a handle onto the real disk, and the signature question and Save a copy
end to end (an unedited save
rewrites nothing, so it never fires; Vitest covers it, E2E waits for T-226's edits).

## Zero-network

`e2e/zero-network.spec.ts` is the web counterpart of `tools/zero-network-guard`.
After the app reports the engine ready, any request fails the test, including
requests from the worker (it opens, renders and saves — a Blob download is not a request). The test also checks that it can see the worker's own
`pdfium.wasm` fetch during load, so in every browser it is shown to be able to
see what it guards.
