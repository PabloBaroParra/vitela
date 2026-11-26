# @vitela/core

Vitela's PDF engine for the browser: the Rust core (`core/pdf-wasm`) and
pdfium, both as WebAssembly, running in a module Web Worker behind a typed,
framework-agnostic client. No server: documents never leave the page.

Today the engine opens (with or without a password), renders pages to RGBA,
moves, rotates, removes and inserts blank pages (with undo/redo),
searches, selects text, and saves the document back to bytes.

## Install

```bash
npm install @vitela/core
```

Published from `npm-v*` tags of the Vitela repository
([`npm-release.yml`](../../.github/workflows/npm-release.yml)), with provenance.

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
  `selectWord`, `annotations`, `comments`, `formFields`, `freeTextLayout`,
  `stampPlacement`.
  An edit answers with the document as it is now (`EditedDocument`: page
  sizes, page ids, blocks, undo/redo state, the session's annotations and the
  id of the one it added, the form fields, and `paintRevision`: the newest
  page-content step still applied, 0 as opened — a viewer that keeps bitmaps
  by page id and turn adds it to its key, so a watermark and its undo repaint
  the pages), and the engine has already refreshed its preview
  when the step changed the pixels, so the next render shows it.
  `applyEdit` takes a page edit (`move`/`rotate`/`remove`, or `insertBlank`: one
  A4 page, `portrait` or `landscape`, at a position) or an annotation
  edit (`AnnotationEdit`: add markup, a shape, ink, a note, free text or an
  image stamp; move, resize, recolour, retext or remove one by id), each one
  undo step. Annotation edits change no pixels — the preview leaves the
  session's annotations out — so the UI draws them itself from
  `EditedDocument.annotations` (the same list `annotations(handle)` answers),
  and they cost no preview refresh. One markup box is one annotation: mark up
  a selection over three lines with three edits. `DocumentInfo` says whether
  annotating is allowed (`annotationEditingAllowed`); a refused edit rejects
  with `EditNotPermitted`, free text Helvetica cannot show with
  `EncodingGap`, an undecodable stamp with `InvalidImage`, an unknown id with
  `AnnotationNotFound`. `comments(handle)` lists the file's own comments
  (read-only) and the session's notes; `freeTextLayout` lays text out the way
  a free-text box will draw it; `stampPlacement` sizes a dropped image. A
  stamp's image is copied to the worker, not transferred, and the engine
  never hands it back: keep it to draw the stamp.
  Forms: `formFields(handle)` lists the document's AcroForm fields
  (`FormField`, by `kind`: `text` with its `value`, `multiline` and
  `maxLength`; `checkbox` with `checked`; `radio` with its `buttons`, each an
  export value and its own box, and the `selected` one; `dropdown` with its
  `options`, `editable` and `selected`), the opened file's own included, so
  the list is not empty at open; `EditedDocument.formFields` is the same
  list after every step. Each has an `id`, its `/T` `name`, its `pageIndex`
  and its widget `rect` on the page as drawn (to scroll a field into view).
  List boxes, push buttons and signature fields stay in the file but are not
  listed. `applyEdit(handle, { kind: 'setFieldValue', fieldId, value })`
  fills one in as one undo step, `value` being `{ kind: 'text', text }`,
  `{ kind: 'checked', checked }` or `{ kind: 'choice', option }` (an export
  value for a radio group; `null` chooses nothing). Unlike an annotation
  edit, a fill changes the pixels — pdfium paints the field — so the engine
  refreshes its preview inside the same request, and on the fill's undo and
  redo: the next render already shows the value. `DocumentInfo` says
  whether filling is allowed (`formFillAllowed`, the same permission bit as
  annotating); a refused fill rejects with `EditNotPermitted` (asked
  first), an unknown id with `FormFieldNotFound`, and a value the field
  cannot hold (the wrong kind, a choice it does not offer, text past its
  `maxLength`) with `UnsupportedOperation`. Filling a field with the value
  it already has is still a step: compare before you send.
  Adding other PDFs takes two steps: `prepareImport(bytes, password?)` opens
  one source (a locked one rejects with `PasswordRequired`/`WrongPassword`
  until given its password), and `importPrepared(handle, ids, index)` adds
  every prepared source at once as one undo step (`ImportedDocument`: the
  edited document plus each source's block id); `discardImport` drops a
  source a pick abandoned.
  Compress: `compressPresets()` lists the presets (`lossless`, `balanced`,
  `small`, the core's own order); `compressionRefusal(handle)` says why a
  document cannot be compressed (`CompressRefusal`: today only
  `encryptedDocumentNotRewritable` — the web keeps a document's protection —
  with the core's message) or `null`; `compressionInvalidatesSignatures(handle)`
  is `true` for a signed file even with nothing edited (a compressed save is
  always a full rewrite). `saveCompressedToBytes(handle, preset,
  invalidateSignatures)` answers a `CompressedDocument`: the PDF with every
  edit (the bytes are transferred), `beforeBytes` (the ordinary save's size,
  not the opened file's), `afterBytes`, `savedBytes`, `reduced` and
  `refusals`. When `reduced` is false the bytes are the ordinary save: there
  is no smaller copy. It records no edit, and rejects with
  `SignaturesWouldBeInvalidated` unless told the user agreed.
  Protect: `protectionRefusal(handle)` says why a document cannot be
  protected (the core's clause: today only a document whose permissions
  withhold modification — the Windows and Android gate) or `null`;
  `protectionInvalidatesSignatures(handle)` asks whether the protected copy
  breaks a signature. `protectToBytes(handle, { openPassword,
  permissionsPassword }, invalidateSignatures)` answers the PDF with every
  edit under new AES-128 protection, every permission granted (the bytes are
  transferred). Both passwords are required and must differ — the core's
  rule: it rejects with `InvalidPasswords` (`INVALID_PASSWORDS`) and its
  sentence, so the UI need not repeat it. It also rejects with
  `EditNotPermitted`, `SignaturesWouldBeInvalidated` (unless told the user
  agreed) and `SaveFailed`. It records no edit, the open document's own
  protection does not change, and the engine keeps neither password.
  Document properties: `documentProperties(handle)` answers the six text keys
  of `/Info` (`title`, `author`, `subject`, `keywords`, `creator`,
  `producer`; `null` for an absent key) as the next save would write them —
  the last pending change's, else the file's — however restricted the
  document is. `metadataRefusal(handle)` says why they cannot be changed (the
  core's clause: the document's permissions withhold modification, the gate
  every native shell asks) or `null`. A change is an ordinary edit:
  `applyEdit(handle, { kind: 'setDocumentProperties', properties })` records
  one undo step (an empty or `null` key is removed, the creation and
  modification dates are kept, the values the document already has record
  nothing) and rejects with `EditNotPermitted` on a refused document; Save
  writes it into `/Info`. No XMP stream is written, as on the native shells.
  Watermarks: `watermarkDefaults()` answers the core's starting looks (text:
  size in points, opacity, angle, over the content; picture: scale, opacity,
  angle, behind the content), and `watermarkRefusal(handle)` says why no
  stamp can be added (the core's clause: the document withholds
  modification, or its encryption cannot survive the rewrite) or `null`.
  `applyEdit(handle, { kind: 'addWatermark', pages, mark, opacity,
  rotationDegrees, behind })` stamps `mark` — `{ kind: 'text', text,
  fontSize }` in Helvetica and the core's grey, or `{ kind: 'picture', image,
  scale }` with PNG or JPEG bytes — on the 0-based `pages`, as one undo step
  that refreshes the preview and that Save paints into each page's content
  stream. It rejects with `EditNotPermitted` (or `UnsupportedOperation`) as
  `watermarkRefusal` says, `PageIndexOutOfBounds`, `EncodingGap`,
  `InvalidImage`, or `UnsupportedOperation` for blank text, a repeated page
  or a look outside the core's ranges; nothing is recorded then. A page whose
  content stream the core cannot rewrite (a chain of filters) fails the
  preview refresh, and the stamp is taken back out (`SaveFailed`).
  Extract and Split: `extractionRefusal(handle)` says why no page can leave
  the document (the core's sentence: its `/P` withholds extraction — bit 5,
  not page assembly — or its encryption cannot survive the rewrite, e.g.
  opened with only one of its two passwords) or `null`;
  `extractSourceIsSigned(handle)` says whether the file as opened is signed
  (an extracted copy's signature no longer verifies). `extractPages(handle,
  pages)` answers a new PDF of `pages` (zero-based, ascending, unique; the
  current order) with every pending edit, protected as the source is (the
  bytes are transferred); it rejects with `InvalidPageSelection`,
  `PageIndexOutOfBounds`, `UnsupportedOperation` and `SaveFailed`, and
  records no edit. `planSplit(cuts, totalPages, documentName)` answers the
  `SplitPart`s (`{ first, last, fileName }`, `report-part1.pdf`…) a split
  after the one-based pages `cuts` (`"3,7"`) makes, by the core's grammar and
  names (rejects with `InvalidPageSelection` and a sentence: one page, no
  cut, a cut after the last page); each part is then one `extractPages`.
  Export images: `imageExportFormats()` (`png`, `jpeg`; there is no JPEG
  quality — the core's encoder has none), `parsePageSelection(input,
  totalPages)` reads `"1-3,7"` into zero-based pages by the core's grammar
  (rejects with `InvalidPageSelection`, whose message is a sentence for the
  person who typed it), `pageImageFileNames(name, pages, totalPages, format)`
  names each file (`report.pdf` page 3 of 12 as JPEG is `report-03.jpg`), and
  `firstPageTooLargeToExport(handle, pages, dpi)` names the first page over
  the raster ceiling (32 Mpx), or `null`. An export is a snapshot of what a
  save would write now — every pending edit, annotation and fill drawn in,
  no signature question since nothing is written — opened with
  `openImageExport(handle)` — it answers `{ handle, pageCount }`, the
  snapshot's own page count, which is what to plan the export against
  (rejects with `UnsupportedOperation` when the document withholds
  extraction) — read one page at a time with
  `exportPageImage(export, pageIndex, dpi, format)` (the encoded file,
  transferred; rendered by the core's renderer and encoded by its
  `encode_rgba_image`, not by a canvas) and freed with
  `closeImageExport(export)`. Close it however the export ends: the snapshot
  holds the engine's memory until then. It records no edit.
  `terminate()` stops the worker for good (every open document goes with it):
  pending and later calls fail with "the engine was terminated", and so does
  a `ready` still pending. Calling it again does nothing.
  Its constructor is public, but wrapping a port yourself is
  not a supported setup: the worker answers nothing until it receives the
  init message that `createEngine` sends.
- `EngineError`, `errorKind`, `errorMessage`, and the kind constants
  `PASSWORD_REQUIRED`, `WRONG_PASSWORD`, `TEXT_EXTRACTION_DENIED`,
  `SIGNATURES_WOULD_BE_INVALIDATED`, `EDIT_NOT_PERMITTED`, `ENCODING_GAP`,
  `ANNOTATION_NOT_FOUND`, `INVALID_IMAGE`, `FORM_FIELD_NOT_FOUND`.
- Types: `Engine`, `DocumentHandle`, `DocumentInfo`, `OpenedDocument`,
  `Edit`, `PageEdit`, `AnnotationEdit`, `MarkupKind`, `FormEdit`,
  `FieldValue`, `WatermarkEdit`, `WatermarkMark`, `WatermarkDefaults`,
  `EditedDocument`, `Annotation`, `Comment`, `FormField`,
  `RadioButton`, `FreeTextLayout`, `FreeTextLine`, `Rgb`,
  `PageRect`, `DocumentBlock`, `BlockSource`, `PreparedImport`,
  `ImportedDocument`, `PageSize`, `PagePoint`, `PageRects`, `RenderedPage`,
  `SearchHit`, `TextSelection`, and the protocol types `MessagePortLike`,
  `RequestEnvelope`, `WorkerMessage`.

All geometry is in PDF points on the page as drawn (top-left origin,
`/Rotate` applied) — annotations, comments and form fields too, so a selection's rects
can be marked up as they come; see the doc comments in
`dist/core/types.d.ts`. The one exception is a free-text box's layout, which
is in the box's own frame and turns with the page.

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
