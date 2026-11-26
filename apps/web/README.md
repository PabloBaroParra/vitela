# Vitela web shell (`apps/web`)

Batch 26. No server: the PDF engine is WASM running in a Web Worker, the UI
is TypeScript + React (Vite). The user's PDF never leaves the browser.

## Layout

```
src/
  main.tsx          boot only: starts the engine (createEngine), loads the
                    editor's stylesheet, renders App
  app/              shell layout and the app's own styles (page frame,
                    landing, tools)
  features/
    tools/          tool registry, routes, home and tool pages
  ui/               landing pieces (header, hero, footer, tool cards)
  test/             shared test fakes (Vitest only)
e2e/                Playwright flows (production build); pdf.ts builds test PDFs
  host/             embed host fixture page (own Vite build, port 4174)
```

## The editor

The editor UI lives in the [`@vitela/viewer`](../../packages/vitela-viewer)
package (npm workspace, B26 T-244): `DocumentWorkspace` and everything under
its `src/features/` (`document`, `viewer`, `render`, `search`, `text`,
`annotations`, `organize`, `save`), `src/platform/`, the `ui/` pieces the
editor uses, `useEngineStatus`, and the stylesheet (`@vitela/viewer/style.css`:
tokens, base, viewer, organize, dialogs). The feature paths named below are
in that package. Like `@vitela/core`, the app consumes its built `dist/`, so
every script below builds both packages first (`npm run build:packages`).

The stylesheet is scoped to the editor's root class, `.vitela` (B26 T-245):
tokens, colour scheme and font live on `.vitela`, element resets sit under
`:where(.vitela)`, and the editor fills its container instead of the viewport.
The app puts `class="vitela"` on `<html>` (`index.html`), so the whole page —
site header and landing included — reads the same tokens as before.
`e2e/embed.spec.ts` checks the scope on a host page with aggressive styles of
its own (`e2e/host/`, built and served on port 4174 by Playwright's second web
server, never part of `dist/`).

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
npm test              # builds both packages, then the app's Vitest
npm test -w @vitela/core   # the engine's own Vitest (includes the real-pdfium test)
npm test -w @vitela/viewer # the editor's own Vitest
```

```bash
npm run build         # builds both packages, type-checks, production build into dist/
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

## Annotations

`features/annotations/`, in the viewer.

- **Layer:** each `PageSlot` mounts an `AnnotationLayer` between the bitmap
  and the text layer: one SVG whose `viewBox` is the page's drawn size in
  points, so the engine's snapshot (`EditedDocument.annotations`, boxes and
  points on the page as drawn) goes in as it is and the zoom is only the SVG's
  CSS size. It draws every kind the snapshot carries: markup (highlight at
  45 % with `multiply`, underline/strikeout as a 1.5 pt rule), shape and ink
  (1 pt), a note icon (its text as the tooltip), free text from the core's
  layout (Helvetica, the box's own frame turned with the page) and a dashed
  placeholder box for a stamp (the engine hands no image back). The session's
  annotations are not in pdfium's bitmap until a save writes them; the file's
  own annotations are, so a freshly opened document has an empty layer.
- **Turned pages:** an underline follows the PDF rect's bottom edge, which a
  quarter turn clockwise takes to the drawn left (`markupRule`); the saved file
  draws it there too. Pure geometry in `geometry.ts`, with tests.
- **Markup:** Highlight, Underline and Strikeout in the text toolbar act on
  the selection. The core's markup has one box (no `/QuadPoints`), so each
  line of the selection is its own `addMarkup` and its own undo step
  (`markupEdits`), sent in order. The buttons are off while any edit, undo
  or redo runs, and a markup whose page was moved under it (another `PageId`
  or turn at its position) marks nothing. The selection is dropped only once
  every line landed, and only if it is still the one that was marked.
  Colour: the native shells' default (255, 220, 0).
- **Undo / Redo** in the viewer's toolbar, and Ctrl/⌘+Z, Ctrl/⌘+Shift+Z and
  (not on a Mac, where ⌘+Y is History) Ctrl+Y anywhere on the page except in
  a text field or behind a modal dialog (`useUndoKeys`; Organize reads the
  same keys through the shared `document/historyKeys.ts`). They
  go through the engine (`undo`/`redo`), which refreshes the preview only for
  a page step, and take back any step — also a page edit made in Organize.
  Steps run in the order asked for: an undo waits for a markup still landing.
  Every answer goes to `useDocument.revise` (which keeps `annotations`) and
  marks the document dirty.
- **Positions follow pages:** each slot renders by its page's `PageId` + turn
  (`usePageRender`'s `pageKey`), so an undo that puts another page at a
  position repaints it (without the old bitmap) and drops the selection and
  search matches.
- **Permission:** `annotationEditingAllowed` (the core's rule: bit 6, or the
  owner password) is known at open; without it the markup buttons are
  `aria-disabled` and described by a note saying why. The core refuses anyway
  (`EditNotPermitted`), and that refusal turns them off too.
- **Drawing tools** (`Draw` toolbar): Select (the default: the text layer has
  the pointer) / Ink / Shape / Note / Free text, `aria-pressed`, plus a colour
  for ink and shapes (a five-swatch palette starting at the core's default
  yellow; notes and text boxes have no colour in the core). An armed tool
  mounts a `DrawingLayer` on top of every page slot that takes the pointer
  (pointer events, captured for the gesture; the text layer gets no input and
  the selection is dropped) and previews the stroke or box in page points; a
  cancelled pointer or a lost capture drops the gesture.
  Escape anywhere — not in a text field or behind a dialog — goes back to
  Select; a tool otherwise stays armed (several strokes make a drawing).
  Geometry is pure (`drawing.ts`, with tests): the pointer divided by the zoom
  is already a point on the page as drawn, and the core turns it into PDF
  space whatever the `/Rotate`. Ink is one `addInk` per stroke (points closer
  than 0.5 pt dropped, kept on the page); a shape is the traced box, or a
  144 × 36 pt box for a click; a note is a 24 pt box at the click; a text box
  is the traced box (at least the core's 16 × 17.8 pt minimum) or 200 × 50 pt
  for a click — the native shells' sizes, with an 8 pt drag threshold. Both
  text-box sizes are the box's own text frame, so on a `/Rotate 90`/`270` page
  the drawn box has them sideways (`drawnSize`). Note and Free text open a
  modal dialog (`TextDialog`, its text area focused; Ctrl/⌘+Enter adds;
  Cancel and Escape wait while Add is in flight, since the edit is already
  queued, and a dialog the browser closes on its own — Chromium's second
  Escape in a row — opens again while its draft is shown); the text box's dialog previews the core's layout
  (`freeTextLayout`, asked when typing pauses, in the box's own frame) and
  shows at once a character Helvetica cannot show. Every drawn annotation is
  one undo step through `useAnnotationEdits.add`: refused while a step runs
  (a stroke or shape refused that way says so in the viewer's notice, via
  `notify`), and checked against the page the gesture began on (`pageKey`). An
  `EncodingGap` from the core keeps the dialog open with its message; any
  other failure closes it and shows in the viewer. The tools are
  `aria-disabled` (Select excepted) while a step runs and, with a note saying
  why, when the document forbids annotations.
- **Image stamps** (T-239): **Image** in the `Draw` toolbar opens a PNG/JPEG
  picker (a hidden `<input type=file>` labelled "Stamp image"); once the file
  reads it arms the `stamp` tool, and each click on a page stamps that image
  there (the tool stays armed; Escape goes back to Select). Only the latest
  pick is kept, and it arms the tool only if no tool was chosen (nor Escape
  pressed) while it loaded. A PNG or JPEG **dropped on a page** is stamped at
  the drop point (kept on the page, like a click) in any tool mode: the page
  slot claims the drop (`preventDefault`), so the workspace's window-wide
  listener opens nothing; a drop with a PDF in it is still the workspace's, to
  open. A file over `MAX_STAMP_BYTES` (20 MB) is refused before it is read;
  then the bytes are checked by their signature before the core sees them
  (`stamps.ts`, pure: anything else is refused with a notice; a file that does
  not read gets its own notice). The core does
  all the placing: `stampPlacement` gives the box (longest side 144 pt, the
  image's own proportions, its top-left corner at the anchor — on a page turned
  90° that is the drawn top-right corner) and `addStamp` adds it, both inside
  one step of `useAnnotationEdits.add` (which takes a function that builds the
  edit in the step): one undo step, and the page is busy from the click, so a
  second click or drop meanwhile is refused before it asks the core anything,
  like a second stroke.
  The engine never hands the image back, so `useStamps` keeps it as a `data:`
  URL (no request, nothing to revoke) by the `addedAnnotationId`:
  `add`'s `onAdded` gets the id in the same step, before the edited document is
  reported, and the URL goes to `OpenDocument.stampImages`
  (`useDocument.keepStampImage`), so it survives undo/redo (a redone stamp
  keeps its id; ids are never reused) and a switch to Organize and back. The
  layer draws it as an SVG `<image>` filling the box's own frame
  (`freeTextFrame`: turned with the page, like the saved appearance); a stamp
  without a kept image keeps the dashed placeholder box. `InvalidImage`,
  "not allowed" and "another step was running" show in the viewer's notice.
- **Editing an annotation** (T-240): in Select, a press on one of the session's
  annotations selects it (`AnnotationEditLayer`: an invisible button per
  annotation over the text layer, inert everywhere else, so a press beside
  them still selects text and lets go). Drag moves it, its corner handles
  resize it (a stamp in proportion; ink cannot be resized), and the floating
  `Annotation` toolbar has Move to…, Resize…, Edit text… (text boxes), the
  palette (markup, shapes, ink) and Delete. From the keyboard: Tab reaches it,
  Enter selects, arrows move by 1 pt (Shift 12), Alt+arrows resize, Delete
  removes, Escape lets go; on a focused handle the arrows move that corner.
  The keys belong to the selected annotation's own elements, so the arrows
  still scroll otherwise. Every change is one undo step through
  `useAnnotationEdits.change`, which queues and builds each edit from the
  annotation as the previous step left it (`editBuilds.ts`); a change that
  changes nothing records nothing, and a drag is dropped, with a notice, if the
  annotation or its page changed under it. Dialog lengths are points from the
  top-left corner of the page as drawn. A focused control that goes away hands
  the focus to the pages (`useKeepFocus`).
- **Annotation navigation and comments** (T-241, `features/comments/`): the
  `Annotations` toolbar's arrows go to the previous/next annotation, page by
  page (on each, the comments the file carries, then the session's annotations
  in paint order), wrapping at both ends, and scroll it into view from the
  layout (`useReveal`, shared with search), clear of the floating `Annotation`
  toolbar (one sideways-scrolling row in a narrow editor). One of the session's is selected
  when it can be edited; anything else (a file's comment, annotating forbidden,
  a tool armed) gets a ring (`AnnotationRing`) until a press on a page or
  Escape there. **Comments** opens a panel (beside the pages; under them when
  the editor is narrower than 700px, a container query) with the core's
  `comments`: each row goes to its comment, **Read note** shows all of it
  read-only, and so does "Read note…" in the `Annotation` toolbar of a note.
  `useComments` asks again after every edited document, one request at a time,
  and shows only the answer to the latest; a failure says why and offers Try
  again, and marks the Comments button while the panel is closed. None of it needs permission to annotate. A file's date is shown as
  written, in its own time zone.
- **Forms** (T-243, `features/forms/`): **Forms** (beside Comments; one side panel
  at a time) lists the AcroForm fields by page — text, checkbox, radio group and
  dropdown — from the core's `formFields` (asked once at open; after that every edited
  document brings them). Text is filled in when the field is left (Tab, a click away) or
  on Enter in a one-line field; Escape puts the document's value back. A checkbox, a radio
  button or a dropdown choice is filled in as it changes. Each fill is one undo step through
  the viewer's edit queue (`useAnnotationEdits.apply`), and a value the field already holds
  is never sent (the core would record it as a step). pdfium paints fields into the bitmap,
  so each page gets a `paintKey` from its fields' values and is painted again when they
  change. Focusing a field scrolls it into view (`useReveal`) and rings it on its page.
  Where `formFillAllowed` is false the panel is read-only and says so; the core's refusal
  of a value is shown under its field.

E2E fixtures come from `tests/fixtures/` (the encrypted one's passwords are in
its README). Documents the corpus lacks — 60 pages, a `/Rotate 90` page, text
at known positions, an RC4-encrypted PDF that forbids copying — are built in
memory by `e2e/pdf.ts` and never written to disk; so are the PNGs the stamp
flows drop and pick (`e2e/png.ts`).

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
prompt and a handle onto the real disk. The signature question and Save a copy
are covered end to end by `e2e/organize.spec.ts` (a deletion on a signed PDF).

## Organize

`features/organize/`, reached with the **View / Organize** switch in the bar
(every document opens in View; each screen mounts afresh on a switch).

- **Engine:** `applyEdit(handle, edit)` (`move` / `rotate` / `remove` of a run
  of pages), `undo`, `redo`. Each answers with the edited `DocumentInfo` plus
  `canUndo`/`canRedo`, and the worker has already run the core's
  `refreshPreview` inside the same request, so no render can land between an
  edit and its refresh. If the refresh fails the step is taken back.
  `useDocument.revise` takes the answer (only for the document still on
  screen, which also gates `markDirty`).
- **Selection** is one contiguous run (`selection.ts`), so every operation is
  one core command and one undo step. Click selects, Shift+click extends.
- **Keyboard:** the grid is a listbox with `aria-activedescendant`. Arrows /
  Home / End move the focus (Shift extends), **Ctrl+Shift+arrow** (Cmd+Shift on
  a Mac) moves the selected pages, Delete/Backspace removes them, Ctrl+Z /
  Ctrl+Shift+Z / Ctrl+Y undo and redo. The toolbar has the same actions.
- **Drag:** pointer events, not HTML5 drag-and-drop; the drop slot comes from
  the grid layout (`dropSlotAt`, in front of the first card whose middle is
  right of the pointer).
- **Virtualized:** only the rows within a viewport of the visible area are
  mounted, so only they render.
- **Thumbnails** (`thumbnailCache.ts`) are kept by the core's `PageId` + turn
  + DPI, never by position: a move or a deletion renders nothing again, a
  rotation changes the key, an undo finds the old bitmap. No new render starts
  while an edit is in flight (the UI's page order is stale until it answers).
- **Permission:** `documentAssemblyAllowed` (the core's rule) is known at open;
  without it the screen shows why and every edit is off. The core refuses
  anyway (`EditNotPermitted`), and that refusal blocks the screen too.
- **Pages / Documents** (switch in the toolbar): the Documents view shows one
  card per block — a contiguous run of pages from one PDF, as the core derives
  it (`DocumentInfo.blocks`, `derive_blocks`) — named after its file ("Part n"
  when a PDF is split). The grid, drag, keyboard and selection are the same
  code over *units* (`units.ts`): a run of blocks is a run of pages, so moving,
  rotating or deleting a block is the same single page command and undo step.
  Switching views keeps the same pages selected.
- **Add PDFs** (picker, several files, or files dropped on the screen):
  `useImport` prepares each file in turn (`prepareImport`), asks a locked one
  for its own password with the same dialog as opening ("…to add its pages"),
  then adds the whole pick after the last page with one `importPrepared` —
  one undo step. A cancelled password, a refused file or leaving the screen
  abandons the pick and discards what was prepared. A drop on Organize is
  claimed (`preventDefault`), so the workspace's window-wide drop opens
  nothing. The file names live on the open document (`sourceNames`, by the
  block source id the import reported); the engine never sees them.
- **Import permission:** `DocumentInfo.importRefusal` (the core's
  `import_refusal`: modify-contents and assembly permissions, and whether the
  encryption survives the rewrite) turns Add PDFs off with the reason as its
  tooltip; the core asks again on `importPrepared`.

## Zero-network

`e2e/zero-network.spec.ts` is the web counterpart of `tools/zero-network-guard`.
After the app reports the engine ready, any request fails the test, including
requests from the worker (it opens, renders and saves — a Blob download is not a request). The test also checks that it can see the worker's own
`pdfium.wasm` fetch during load, so in every browser it is shown to be able to
see what it guards.
