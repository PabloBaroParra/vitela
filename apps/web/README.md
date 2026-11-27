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

## Tools menu

The document tools — **Compress**, **Export images**, **Print**, **Protect**,
**Watermark** and **Document properties**, in that order — are one menu
button, **Tools** (`ui/MenuButton.tsx`, the WAI-ARIA "menu button" pattern),
in the workspace bar before Save, in both screens and at every width (T-256).
They have no buttons of their own. A button each no longer fit: in an
embedding host whose own button styles are larger the bar already took 1191
of its 1200 px, and Document properties had to move to the viewer's View
toolbar; one menu is also one code path for the desktop and the phone, where
these tools were already a "More tools" menu. On a phone (≤ 600 px) the
button shows three dots instead of the word (its name stays "Tools"), so the
bar keeps one row. Choosing an item gives the focus back to Tools before the
dialog opens, so every dialog hands it back there; each tool's status line
and dialog stay in the bar, so they survive a switch between View and
Organize. `e2e/phone-bar.spec.ts` checks the single row at 390 px, the DOM
and Tab order, and the menu's keys; `e2e/properties.spec.ts` checks the bar
is one row at 1280 px; `e2e/embed.spec.ts` keeps the host's page in frame.

## Compress

`features/compress/` (T-250), the native Compress tiles' order. **Compress**,
in the **Tools** menu in both screens, first asks the engine the two gates
(`compressionRefusal`, then — only when there is none —
`compressionInvalidatesSignatures`) and opens the dialog with the core's presets
(`compressPresets`, Balanced chosen). An encrypted document is refused there
(the core's sentence, presets and Compress disabled: the web keeps a document's
protection, so there is no "strip it" way past). A signed one says the copy's
signature will not verify, and the button reads **Compress anyway**: confirming
is the consent, there is no second prompt. A failed gate refuses rather than
guessing. The run waits for the queued edits (like Save), then
`saveCompressedToBytes`; only a copy that came out **smaller** is delivered —
always a new file, `<name>-compressed.pdf` (`Placement` `newFile`: a download,
or the host's `onSave`); otherwise the status says there was nothing to gain and
nothing is written. The status reports sizes as "uncompressed → compressed"
(the ordinary save of the session, not the file on disk) in powers of ten, the
percentage truncated, and appends any refusal the core reports. The open
document, its unsaved edits and its undo history are untouched. No predicted
saving is shown: the core cannot answer that without running. On a phone
(≤ 600 px) the bar's controls tighten, so the bar stays one row: a taller bar
takes its height from the pages and the Comments panel under them, whose
first row then hides under its sticky head.

E2E (`e2e/compress.spec.ts`, all three browsers): a scan from
`tests/fixtures/compress` comes out smaller, downloads under its name and
reopens; a deletion made in Organize is in the copy and stays unsaved and
undoable; an encrypted PDF is refused in the dialog with nothing downloaded;
a signed PDF asks, and "Compress anyway" downloads the copy. "Nothing to gain"
is covered by Vitest only: no fixture found reaches it through a real save (the
save itself writes loosely enough that the repack always wins something).

## Export images

`features/exportImages/` (T-252), the native Export images' order: choices, a
plan, then one file per page. **Export images**, in **Tools** in both
screens, opens a dialog with the native shells' three questions: pages (All
pages, Current page — the page the viewer last showed — or a typed range such
as `1-3,7`; typing one picks it), PNG or JPEG, and a resolution from 72 to 400
DPI (150 by default; a cleared box reads as 150). The ceiling is the natives'
too: the core refuses any raster over 32 Mpx and US Letter at 600 DPI is over
it. There is no JPEG quality setting: the core's encoder has none, nor do the
other shells. A document that withholds extraction says so as the dialog opens
and offers only Cancel.

The formats offered are the core's (`imageExportFormats`, PNG chosen first).
Export checks the resolution and the permission, waits for the queued edits
(like Save), then `openImageExport` takes one snapshot of what a save would
write now — so unsaved page edits, annotations and fills are in the images, as
on Android and Windows, while the on-screen preview leaves annotations out —
and answers its page count. The rest is checked against that count while the
dialog is still open (`planExport.ts`): the range by the core's grammar
(`parsePageSelection`, its sentence shown as is), the raster ceiling
(`firstPageTooLargeToExport`) and the names (`pageImageFileNames`: `report.pdf`
page 3 of 12 is `report-03.jpg`). A refusal closes the snapshot and keeps the
dialog open on what was typed. Then each page is rendered, encoded
by the core (`exportPageImage`) and delivered at once as its own file
(`Placement` `newFile` with the image's media type: a download, or the host's
`onSave`). The export stops at the first page that fails, on **Cancel export**
(shown while it runs; the page in hand still goes out) or when the editor
unmounts, and says how many were written; the snapshot is closed however it
ends. Nothing about the open
document changes.

**Several pages are several downloads, not a zip.** No zip writer is in the
dependency tree (neither npm nor Cargo), and adding one would be the only
reason for it; the native shells write one file per page into a folder, and
one download per page is the browser's nearest equivalent. **Blocked
downloads:** a browser can ask once whether the site may download multiple
files, or block them by its settings. A page cannot tell whether a download
was allowed, so "Exported 3 pages" means the files were handed to the
browser; if fewer arrived, allow multiple downloads for the site and export
again, or export one page at a time. That prompt is not covered by the tests
(Playwright accepts every download).

E2E (`e2e/export-images.spec.ts`, all three browsers): the current page (after
scrolling to page 3) as PNG at 144 DPI is one download, `vector_only-3.png`,
1224 × 1584; pages `2-3` as JPEG at 72 DPI are `vector_only-2.jpg` and
`vector_only-3.jpg`, 612 × 792 each (read from the files' headers); a page
turned in Organize and not saved exports landscape and stays unsaved and
undoable; a range past the last page shows the core's sentence and 600 DPI the
range refusal, with nothing downloaded.

## Print

`features/print/` (T-257). **Print**, in **Tools** in both screens, or
Ctrl+P (⌘+P on Apple platforms) while the key is the editor's — the same
scope as the history keys: focus in the editor, or nothing focused after the
user's last click or drop there; on an embedding host's page a key aimed at
the host is left to the browser, which prints the host's page. The key is
also left alone while one of the editor's dialogs is open. Native printing is
the browser's: its dialog picks the printer, copies and paper.

What prints is what a save would write now, as on GTK and Windows (which
print an annotated session from its output snapshot): Print waits for the
queued edits, then `openPrintSnapshot` takes the same snapshot as Export
images, so pending page edits, annotations, fills and watermarks are on the
paper. Unlike the export it asks **no permission**: the core has no rule for
the `/P` print bit and no native shell asks one, so a document that withholds
printing still prints (a decision for the core, not the web, if it changes).
The pages are drawn at 150 DPI, lowered for a long document so that every
page together stays within 128 Mpx (`printDpi`: the frame keeps each page
decoded until the dialog closes, ~512 MB at that budget, about 60 Letter
pages at full resolution), never below 96 DPI: past ~148 Letter pages the
document still prints, at 96. The core caps a print snapshot at 150 DPI
(`exportPageImage` refuses finer), so it is no way around the export's
permission. A page over the raster ceiling at that resolution is refused up
front; otherwise each page is rendered and encoded as PNG by the core
(`exportPageImage`) one at a time, shown as "Preparing page 2 of 5 for
printing…" with **Cancel print** — heard before each page, after the last
one and while the frame decodes its pages one by one: until the dialog opens,
a cancel means no dialog and nothing kept. Only the encoded files are kept,
as blob URLs; the snapshot is closed before the dialog opens. The snapshot
lifecycle and page loop are shared with Export images
(`features/snapshot/`).

The pages go into a hidden `iframe` (`printFrame.ts`, `srcdoc`, laid out at
0 × 0 because a `display: none` frame does not print in Firefox) inside the
editor's root, never elsewhere on a host's page, and the frame's own window
prints: the page around it never does, and no stylesheet crosses either way
(the frame has one stylesheet of its own and nothing else: a constructed,
adopted one, and the frame's own size through the CSSOM, so a strict CSP's
`style-src 'self'` allows both; a `<style>` only where constructed
stylesheets are missing). Each page is one
sheet sized in points from its image at the DPI it was drawn at, a named `@page` per distinct
size, so a page turned a quarter prints landscape. The document title is the
file name without `.pdf` (what the dialog offers when it saves to a file). The
frame and its images go once the dialog reports `afterprint`, or with the
next print, a new document or the editor.

E2E (`e2e/print.spec.ts`, the dialog stubbed by an init script that records
what the frame held): three pages print as three images in order, 1275 × 1650
px and the turned one 1650 × 1275, on `612pt 792pt` and `792pt 612pt` sheets;
in Chromium the frame's document printed to PDF has those two page sizes; a
pending watermark adds ink to the printed page and the document stays
unsaved; Ctrl+P with focus in the app prints the frame and never the app; a
document whose `/P` withholds printing and copying still prints; embedded,
Ctrl+P on a host button prints nothing of the editor's, and in the editor
prints one image from a frame inside `#host-frame` with only its own
stylesheet.

## Protect

`features/protect/` (T-251), the native Protect commands' order: the gates,
two passwords, then the protected file. **Protect**, in **Tools** in both
screens, first asks the engine the two
gates (`protectionRefusal` — a document whose permissions withhold
modification, the Windows and Android gate — then, only when there is none,
`protectionInvalidatesSignatures`) and opens **Protect with a password**: the
PDF's two password roles, as on every shell — **Password to open the
document** and **Permissions password**, which lifts the permissions — with
no confirmation fields (none of the native shells has them). A refused
document says why and offers only Cancel; a signed one warns that its
signature will no longer verify and the button reads **Protect anyway**:
confirming is the consent. A failed gate refuses rather than guessing.

Every password rule is the core's (`pdf_save::protect`): the dialog sends what
was typed and, when the core refuses (one missing, or both the same:
`InvalidPasswords`), shows its sentence with the dialog still open and what
was typed still there. The passwords live only in the dialog's own fields
(`type="password"`, `autocomplete="new-password"`), never in React state; they
cross to the worker in the one protect request and are kept nowhere, and the
fields are emptied when the dialog goes. The run waits for the queued edits
(like Save), then `protectToBytes`: AES-128, every permission granted, the
unsaved edits in. The copy is always a new file, `<name>-protected.pdf`
(`Placement` `newFile`: a download, or the host's `onSave`).

**Not reopened in place.** The native shells reopen the protected file as the
session (under both passwords). The web cannot: the copy is a download, not
the file the session came from, so the open document — unprotected, with its
unsaved edits and undo history — stays as it was, and the status says so. To
work on the protected copy, open it: the app asks for its password, and either
one opens it.

E2E (`e2e/protect.spec.ts`, all three browsers): a page deleted in Organize and
protected downloads as `vector_only-protected.pdf` with an `/Encrypt`, the edit
still unsaved and undoable; the copy refuses a wrong password, opens with the
open password (2 pages, painted, Organize's Delete enabled: every permission
granted) and with the permissions password; a missing and a repeated password
show the core's sentences with nothing downloaded, and the dialog forgets what
was typed; a signed PDF asks and "Protect anyway" downloads a copy that opens;
`aes_128_user_and_owner.pdf` opened with its user password (its `/P` withholds
modification) is refused in the dialog. `e2e/phone-bar.spec.ts` checks Protect
is listed in Tools at 390 px, its dialog fits, and the focus comes back to
Tools.

## Document properties

`features/properties/` (T-255), Android's **Document properties** dialog (the
Windows and Linux shells show the same keys in a panel): **Title**,
**Author**, **Subject**, **Keywords**, **Creator** and **Producer**, as the
next save would write them (`documentProperties`: the last pending change,
else the file's `/Info`). The dates are neither shown nor edited (as on
Android); every change keeps them. **Document properties** is in **Tools**,
in both screens (T-255 first put a Properties button at the end of the
viewer's View toolbar, because the bar had no width left in an embedding
host; T-256 folded every tool into the menu, see "Tools menu").

**Apply** waits for the queued edits, reads the properties again (an undo may
have landed) and records only a real change: an emptied box is an absent key,
never `""`, and boxes that say what the document has close the dialog with
nothing recorded. The change is one step in the document's history (Undo and
Redo, from the toolbar or the keyboard, take it back and forth), marks the
document unsaved and is written by Save; the dialog hands its apply to the
save, so a save asked for meanwhile waits for it. A document whose permissions
withhold modification (`metadataRefusal`, the gate every native shell asks)
shows its properties read-only with the core's sentence and only **Close**. A
refused apply keeps the dialog and what was typed. No XMP stream is written
(no shell syncs one).

E2E (`e2e/properties.spec.ts`, all three browsers): title and author typed
into a new PDF, applied (unsaved, focus back on Tools), undone and redone
(the dialog shows each state), saved — the downloaded file's `/Info` has them
(`Ana Peña` included) and no `/Subject` — and reopened, where the dialog shows
them and an unchanged Apply records nothing; a PDF whose `/P` withholds
modification, opened with its user password, is read-only with only Close; on
a phone, Tools opens it, it fits 390 px and the focus comes back to Tools; at
1280 px the bar is one row, View still shares its row with Text and
Annotations with Draw. `e2e/phone-bar.spec.ts` lists it in Tools.

## Watermark

`features/watermark/` (T-256), the native **Add watermark** (Android's
dialog and labels; GTK's and Windows' ranges): the same text or picture
stamped on the pages the reader picks, as **one** undo step. **Watermark**,
in **Tools** in both screens, asks the engine the core's starting look
(`watermarkDefaults`: diagonal grey text at 60 pt, 30 %, 45°, over the
content; a picture upright at half the page, behind it) and the gate
(`watermarkRefusal`: the modify-contents permission, then whether the
rewrite a stamp forces can keep the document's encryption — the gate every
native shell asks). A refused document gets the core's sentence and only
**Close**.

The dialog asks **Text** or **Picture**; the text (DRAFT to start) or a PNG
or JPEG (**Choose picture…**, its name and size shown; decoding is the
core's); **Size** (8–200 pt, or 5–100 % of the page), **Opacity** (5–100 %)
and **Angle** (−180…180°) as sliders; **Placement** (over or behind the
page's content); and **Pages** (all, the page the viewer last showed, or a
typed range read by the core's grammar). Switching kind resets only the
angle and the placement to that kind's defaults, as the natives do. No
colour choice and no live preview: no native shell has either. **Add** waits
for the queued edits, resolves the pages against the page count the last
edit left and records one `addWatermark` edit; a character Helvetica cannot
show (`EncodingGap`, with a hint), a picture that does not decode
(`InvalidImage`) or a bad range keeps the dialog open on the draft. The
stamp is page content: it marks the document unsaved, Undo and Redo take it
off and back on every page at once, and Save paints it into each page's
content stream (one shared image object for a picture).

**Repaint in place.** A stamp changes what a page shows while its id and turn
stay the same, which the viewer's bitmaps and Organize's thumbnail cache are
keyed on. Every edit answer now carries the engine's `paintRevision` (the
newest page-content step still applied, from `pdf-wasm`'s preview marks), and
both keys include it: a stamp, its undo and its redo repaint the pages; a
move, a rotation or an annotation still repaint nothing.

E2E (`e2e/watermark.spec.ts`, all three browsers): text on every page of a
three-page PDF (focus back on Tools, unsaved, the page's ink changes and Undo
takes it back, Redo returns it), saved — each page's content has the
`Tj` and a `/Watermark` artifact — and found three times by the app's search
in the reopened file; a picture behind the content of pages `1,3` is one
`/Image` object painted twice (and Add without a picture is refused); text
Helvetica cannot show and a range past the end keep the dialog; a PDF whose
`/P` withholds modification is refused with only Close; in Organize the
thumbnails gain the stamp and lose it on Undo; on a phone the dialog fits
390 px.

## Edit content

`features/content/` (T-258b, T-259…T-263): the mode the page's own text,
images and shapes are edited in — text runs since T-259, images since T-260,
paragraphs and inserted text since T-261, shapes since T-263. **Edit content** is the last item of
**Tools**, a `menuitemcheckbox` (checked while the mode is on; chosen from
Organize it switches to the viewer). The viewer's toolbar rows have no room
for a button of its own: at 1280 px the first row (View, Text) and the second
(Draw, History, Annotations) are full, and at 390 px another row would push
the pages down. The workspace asks the engine once per document whether the
mode may be offered (`contentEditingRefusal`: the modify permission and the
rewrite a content edit forces, then the text permission the reads are
behind). A refused document keeps the item, `aria-disabled`, described by
the core's sentence ("Content editing is not permitted."). While the mode is
on a row under the toolbars holds its status and **Done** (which hands the
focus back to Tools); switching to Organize or opening another document turns
it off too.

While it is on, a layer over each page has the pointer (no text selection,
no annotation picking, the drawing tool back to Select): the text run, image
or shape under it is outlined — runs solid blue, images dashed amber, shapes
dotted green — and a press selects the smallest item under it (a run over a
picture, a rule within 3 pt) and the status line, a polite live region,
names it ("Text “…” selected on page 2.", "Image selected on
page 1.", "Shape selected on page 1."; a run whose font does not say what all
its characters are says it cannot be retyped). Escape on the pages lets the
selection go; an edit that repaints the page does too.

**Text runs (T-259).** With a run selected, Enter or a double-click opens a
camouflaged editor laid over it — in the run's own embedded font when the
browser takes the program (`FontFace`, TrueType / OpenType), else the family
its name suggests, at its size, colour, weight and slant. Enter commits,
Escape cancels, leaving it (a press beside it, a save, leaving the mode)
commits; it is closed a turn later, never inside its own blur. The arrow keys
nudge the run 1 pt (Shift: 10 pt) and a drag of its outline moves it; presses
that come while the step runs add up into one move. Delete or Backspace
removes it. A small toolbar over the run has **Edit text**, **B** / **I** /
**U** (pressed as the core reads the run's look) and **Delete**. Every edit is
one undo step, marks the document dirty, and is what Save writes; the run is
found again by place once its page is read anew. A run whose font does not
say what its characters are says it cannot be retyped. While any step runs no
page is read and a press selects nothing: the parse held may name what the
step is about to change.

**Images (T-260).** With an image selected, eight handles sit on its box: a
corner resizes it keeping its proportions (Shift frees them; the native
shells resize from four free corners), a side changes one dimension, and
the box it would take is drawn while a handle is dragged. A drag of its body
moves it, the arrow keys nudge it 1 pt (Shift: 10 pt), Delete or Backspace
removes it. A toolbar over it has **Replace** and **Delete**: the core is
asked whether the image can be replaced (`checkImageReplacement`) as soon as
it is selected, and Replace opens the file picker only when it said yes
(else it says why); the file is checked like a stamp's (PNG or JPEG, at most
`MAX_STAMP_BYTES`). **Insert image**, in the mode's status row, picks a file
and then arms a click on a page (pressed while armed; Escape or a second
press disarms): the core sizes the image (`stampPlacement`, the longest side
two inches, its top-left corner at the click) and inserts it, and it is
selected where it lands. Each edit is one undo step, marks the document
dirty, and the image is found again by its box once its page is read anew.
A commit made because the editor's page scrolled out does not wait for that
page to be read again (it is not in the parse window): the keys stay the
pages'.

**Paragraphs and new text (T-261).** A run that is part of a paragraph —
stacked lines the core groups as one block (`readParagraphBlocks`, read once
per read of the selected run's page, never while a step runs) — adds **Edit
paragraph** to its toolbar (Shift+Enter on the pages too). It opens a
multi-line editor over the whole paragraph, in its first run's size, colour
and leading, wrapping at the paragraph's width; as in the GTK shell, Enter is
a line break, Ctrl/Cmd+Enter commits, Escape cancels, and leaving it (a press
beside it, a save, leaving the mode, its page scrolling out) commits. The
core retypes the block as one step (`retypeTextBlock`): each typed line is
wrapped to the width, lines it grows are painted like the paragraph's own,
an emptied paragraph is removed, and the first line is selected again.
**Insert text**, in the status row beside Insert image (one armed at a time),
arms a click on a page (Escape or a second press disarms): an empty,
outlined box opens there, hanging from the click in 14 pt Helvetica (the GTK
and Windows shells' default) and turned with the page; Enter or leaving it
records the line as one step (`insertText`; the core picks a fresh font
resource and measures it), Escape or an empty box records nothing, and the
new line is selected where it lands — retypable, movable and deletable like
any run before the document is saved (the core checkpoints). A character
Helvetica cannot show is refused by the core (`EncodingGap`). The draft of
both editors lives in their containers (`useDraftEditor`, shared), not in
the field, so a scroll that unmounts the page keeps it.

**Several at once, and the clipboard (T-262).** A drag from where nothing is
draws a marquee and selects every item it touches (runs, images and shapes,
as the Windows shell gathers them; with Shift or Ctrl/⌘ held it adds to
what is selected), and Shift or Ctrl/⌘ with a press on an item adds it or
takes it out. One item gathered is the ordinary selection, with its toolbar
or handles; several are a group: each outlined as selected inside one dashed
box, and the status line counts them ("3 items selected on page 2."). A drag
anywhere in the box or the arrow keys (1 pt, Shift 10 pt) move them all
together and Delete removes them all, each one step (`moveContentSelection`,
`removeContentSelection`); the group is found again by place once its page
is read anew. **Copy**, **Cut** and **Paste** sit in the status row (Ctrl/⌘
+ C, X and V too while the mode is on and the focus is in the editor, never
in a text field or one of its dialogs; a copy or cut with nothing selected,
and a paste with no clip, are left to the browser). They work on the group
or the one item selected. A copy (`copyContentSelection`) records nothing;
a cut is the copy for a cut (its own permission first) and then the removal
as one step, and keeps the clip only once the removal is in. The clip is the
session's (`clipStore`, held by the workspace above its viewers, as the
native shells keep it on the session), so it pastes onto any page — and into
any document opened after it. A paste is one step (`pasteContent`): with the
keys at the pointer while it is over a page, with the button (or no pointer
on a page) at the place it was copied from on the first page in view, kept
on that page (the Windows shell's rule). What it added is selected once the
page is read again (the items that read `pending` where it landed: a group,
or one image alone as a click selects it), so a drag moves the whole paste
and it stays one step. Every refusal is the core's sentence.

**Shapes (T-263).** A filled shape is hit anywhere inside it, an outline (a
stroked path, a table frame) only within 3 pt of its edges — further in is
its cell, as the native shells decide it. With a shape selected, eight
handles sit on its box and resize it freely (Shift keeps the proportions;
the line width is kept, the core rewrites the coordinates); a rule gets only
the two handles at its ends and stays flat. A drag of its body moves it, the
arrow keys nudge it 1 pt (Shift: 10 pt), Delete or Backspace removes it, and
its toolbar has **Delete**. Each edit is one undo step (`movePath`,
`resizePath`, `removePath`; a second move or resize of the same shape folds
into its step) and the shape is found again by its box once its page is read
anew. While a cut is being read nothing is asked of the selected item (its
toolbar and handles are busy, a double-click opens nothing) and only Delete,
Backspace and the arrows are taken by the pages, doing nothing.

**Only the pages in view and one on each side are parsed** (the Linux shell
froze parsing every page of a 1619-page file), each parse kept by page id,
`paintRevision` and `contentBaseGeneration` and dropped once its page is out
of reach, so memory follows the view, not the document. A parse is also
keyed by the page's rotation (an undone turn keeps the id and the revision),
an answer whose position holds another page by the time it lands is dropped
and read again, and at most two reads are in flight: a page a fast scroll
already passed is never asked for.

E2E (`e2e/edit-content.spec.ts`, all three browsers): a run under the
pointer is outlined around the pointer, a press selects it and the status
names its text, nothing is selected as text, Escape lets it go, the menu
item reads checked and Done turns the mode off with the focus on Tools; zoomed out on a 30-page PDF only the pages in view and the next
are parsed and every farther mounted page waits; the scan of the compress
corpus outlines and selects an image; a PDF whose `/P` withholds
modification keeps the item off with the core's sentence as its
description.

E2E (`e2e/edit-text-runs.spec.ts`, all three browsers): retype in place, undo,
redo, save and reopen reads the new text; a double-click opens the editor,
Escape cancels and a press beside it commits; three Shift+Right presses move
30 pt as one step and a drag moves it further, saved where it was left;
Delete removes it and the reopened file has no run there; Bold toggles and
reads back bold after a save; each undone and redone; a PDF that forbids
modification offers no edit at all.

E2E (`e2e/edit-images.spec.ts`, all three browsers): an image moves by the
arrows and by a drag of its body as one step, saved and reopened where it was
left; a corner handle resizes it in proportion, with Shift freely, one step
with the move, saved and reopened in its new box; Delete and the toolbar's
Delete remove it, undone and redone, and the reopened file paints nothing
there; Replace asks the core, then the picker, and the saved file holds the
new picture (its XObject's `/Width`) in the same box; Insert image arms a
click, Escape disarms it, the inserted image is placed at the core's size,
moved before any save, undone and redone, and saved; a PDF that forbids
modification offers no image edit.

E2E (`e2e/edit-text-blocks.spec.ts`, all three browsers): Insert text arms a
click, Escape at the pages disarms and Escape in the box records nothing; the
box hangs from the click at 14 pt, Enter records the line, it is selected
where it landed, undone, redone, saved and read back on reopening; a line not
saved yet is retyped by the run editor and saved as retyped; a two-line
paragraph opens in the paragraph editor from its second line's toolbar,
Enter adds a line and Ctrl+Enter commits, one step that grows a third line a
leading under the second, undone and redone, and the reopened file reads the
three lines back as one paragraph; Shift+Enter opens the editor, Escape
cancels it and a press beside it commits; a run on one line offers no
paragraph editor and a PDF that forbids modification offers no insert.

E2E (`e2e/edit-clipboard.spec.ts`, all three browsers): a marquee from the
margin gathers a line and an image as a group, Ctrl/⌘+C copies it (nothing
to undo), Ctrl/⌘+V with the pointer over page 2 pastes it there and selects
what it added, a drag moves the paste and it is still one step (undone,
redone), and the reopened file has the line and the image on both pages, the
paste where it was dragged; Cut takes an image off page 1 as one step and
Paste puts it at the same place on page 2, in view (two steps, undone and
redone), saved and read back; Shift/Ctrl+click adds and takes out an item,
two Shift+Down move the group 20 pt as one step, Delete removes a marqueed
group as one step, saved and read back; a clip copied in one PDF pastes into
the next one opened, and Ctrl/⌘+V in the search box pastes nothing on the
page.

E2E (`e2e/edit-paths.spec.ts`, all three browsers): a filled box moves by
two Shift+Down and a drag of its body as one step (undone, redone), saved and
read back where it was left; a corner resizes it freely, a rule (two handles
only) lengthens from its end and stays flat, both saved and read back;
Delete removes the box and the toolbar the rule, two steps undone and
redone, and the saved file paints neither; on a page turned 90° a box moves
and resizes as drawn and is saved there.

## Organize

`features/organize/`, reached with the **View / Organize** switch in the bar
(every document opens in View; each screen mounts afresh on a switch).

- **Engine:** `applyEdit(handle, edit)` (`move` / `rotate` / `remove` of a run
  of pages, `insertBlank` of one page), `undo`, `redo`. Each answers with the edited `DocumentInfo` plus
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
- **Add blank page** (menu button in the toolbar: **Portrait A4** / **Landscape
  A4**, the two pages every native shell offers): one `insertBlank` edit — the
  core's `Command::insert_blank_page`, so the document mints the new page's
  id — placed after the selected pages (after the last page of the selected
  blocks in the Documents view, where it shows as a "Blank pages" block), or
  at the end when nothing is selected; then the new card is selected. One undo
  step. It is an assembly edit that changes the page structure, so the core's
  gate is the one Move and Delete ask (`check_edit`: the assembly permission,
  then whether the encryption survives the full rewrite); without the
  permission the button is off and never opens.
- **Extract** and **Split** (toolbar buttons; `features/extract/`): page
  tools that write **new files** and change nothing about the open document,
  so the assembly permission does not hold them back; like Undo, they are off
  while an Organize edit is in flight. Organize's edit queue is now one Save
  waits for (so do Compress, Protect, Export, Extract and Split), and both
  read the page count as of the last landed edit, not the click's.
  Their gates are the native shells', asked as the dialog opens
  (`extractionRefusal`: the copy/extract bit `/P` 5, then whether the
  encryption survives the full rewrite — a PDF opened with only its user
  password cannot); a refused document says why and offers only Cancel.
  **Extract** opens on the selection (`"2-3"`) in a "Pages to extract" field
  read by the core's grammar (`parsePageSelection`), and downloads
  `<name>-extract.pdf` (Android's and Windows' name). **Split** asks "Split
  after page" (`3,7`, the one mode every native shell has), plans with the
  core (`planSplit`: the parts and their names, `<name>-part1.pdf`…, the same
  on every shell; a one-page document is refused up front) and delivers one
  download per part, in order, as Export images does (**Cancel split** while
  it runs; the part in hand still goes out). The engine is asked for at
  most one part ahead of the one being written, so a cancel stops the work as
  well as the downloads and no more than two parts are held in memory (an
  edit made while a split runs reaches the parts asked for after it). Both wait for the queued edits, so unsaved edits are in the
  files; a protected source gives protected files, and a signed one files
  whose signature no longer verifies (the status line says so). The natives
  write into a folder with one overwrite prompt; a download overwrites
  nothing, so there is none. **Blocked downloads:** as for Export images
  (below), a browser may ask once, or refuse, to download several files; if
  fewer parts arrived than "Split into n PDFs." says, allow multiple
  downloads for the site and split again. The buttons sit in Organize, with
  the other page tools, not in the workspace bar: two more bar buttons
  wrapped the bar onto another row in narrow embeds (the embed host's
  oversized buttons), taking height from the pages; their status lines and
  Cancel split do sit in the bar, so they survive a switch to View.
  E2E (`e2e/extract-split.spec.ts`, all three browsers): pages 2-3 of a
  five-page PDF → `five-extract.pdf` with exactly those two pages (by their
  `/MediaBox` widths), which opens with 2 pages; the selection fills the
  dialog and a pending deletion is in the file, still unsaved; a split after
  2 and 4 → `five-part1..3.pdf` with 2, 2 and 1 pages; a cut after the last
  page refused in the dialog with nothing downloaded; a PDF that withholds
  extraction refused in both dialogs; the Split dialog fits a 390 px phone.

## Zero-network

`e2e/zero-network.spec.ts` is the web counterpart of `tools/zero-network-guard`.
After the app reports the engine ready, any request fails the test, including
requests from the worker (it opens, renders and saves — a Blob download is not a request). The test also checks that it can see the worker's own
`pdfium.wasm` fetch during load, so in every browser it is shown to be able to
see what it guards.

## Installable and offline (PWA)

A production build (`npm run build`) is an installable app that works offline
after its first load. `npm run dev` and Vitest never register the service
worker, so they always run the code on disk.

- **Manifest:** `scripts/pwaPlugin.ts` writes `dist/manifest.webmanifest`
  (`src/features/pwa/manifest.ts`: name Vitela, `start_url` and `scope` `/`,
  `display: standalone`, theme `#1b2a57` = `--accent`, background `#ffffff`)
  and copies its icons into `dist/icons/` from the repo: the brand mark
  (`assets/brand/vitela-app-mark.svg`), the macOS 512 px app icon and the
  Play Store icon as the maskable one. `index.html` gets the manifest link,
  `theme-color`, the icon and `apple-touch-icon` at build time.
- **Service worker:** after the build has written every file, the plugin
  lists `dist/` (`src/features/pwa/precache.ts`: `index.html` as `/`, the
  hashed JS/CSS, the engine worker, both WASM modules, icons and manifest;
  never `sw.js`, `_headers`, `_redirects`, `404.html`, source maps or a
  `.pdf`), names the cache after a SHA-256 of those files' names and bytes,
  and bundles `src/features/pwa/sw.ts` into `dist/sw.js` with both baked in.
  Install precaches everything (~9 MB); activate deletes older builds'
  caches. A navigation to one of the app's routes (home and every tool route,
  as `resolveRoute` resolves them — the paths `public/_redirects` rewrites)
  gets the cached app shell, a precached asset its cached copy, and on a miss
  the network refills the cache (`src/features/pwa/swRules.ts`). Any other
  path goes to the network, so the host still answers it with `404.html`;
  other origins, non-GETs and anything the build did not write are never
  touched either. Hand-written, no Workbox: one precache list, no runtime
  routes, no new dependency. The page registers it after the window's `load`.
- **Updates:** a new deploy is a new `sw.js` (served `Cache-Control:
  no-cache`, `public/_headers`) with a new cache. It installs in the
  background and waits; the page shows "A new version of Vitela is ready."
  with **Reload** (`src/features/pwa/updates.ts`, `UpdateBanner.tsx`).
  Nothing reloads on its own, and while the open document has unsaved changes
  (`DocumentWorkspace`'s `onDirtyChange`) Reload is disabled and the switch
  is refused. The hold counts across tabs: on Reload the waiting worker asks
  every open tab of the app (a `MessageChannel` question each tab answers)
  and refuses to take over while any of them has unsaved changes — or does
  not answer within 2 s — since taking over switches every tab and deletes
  the old build's cache. The refused tab says "Another Vitela tab has unsaved
  changes" and Reload asks again. The banner never strands: once a new build
  controls the page (its own take-over or another tab's) Reload just
  reloads, a waiting build that is replaced takes the banner with it, and a
  take-over dropped because this tab had unsaved changes reloads once they
  are saved. Closing every tab also lets the new build take over.
- **Privacy:** the cache holds only the app's own files. A PDF is opened from
  the disk and never fetched over HTTP, so it never passes through the
  service worker and is never cached; nothing about a document is stored.
  The precache download right after the first load fetches the app's own
  files from its own origin — no user data.
- **Tests:** Vitest for the precache list, the version, the request rules,
  the manifest, the update gating and the banner. `e2e/pwa.spec.ts` (the
  only spec that allows service workers, see `playwright.config.ts`) loads
  the app, waits for the precache, goes offline, proves the worker's own
  requests fail (a precached file it no longer holds; not in Firefox, whose
  offline mode answers them from the HTTP cache), reloads, opens a fixture
  and paints it, opens `/organize`, checks no PDF was cached, and that an
  unrouted path is not the shell; it also checks the manifest, its icons and
  `sw.js` are served.

## Content-Security-Policy

`public/_headers` is the one source of truth for the response headers, the
policies included, and `vite preview` serves it too (`scripts/hostHeaders.ts`
reads the built copy with `src/features/hosting/headers.ts`, Cloudflare
Pages' rules: every matching rule in file order, `! Name` detaches), so every
e2e spec runs under the policy that ships. The dev server has none (its hot
reload is inline script).

- **The page:** `default-src 'none'; script-src 'self'; style-src 'self';
  img-src 'self' blob: data:; connect-src 'self'; worker-src 'self';
  manifest-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action
  'none'; object-src 'none'`. The build emits no inline script; the print
  frame styles itself through the CSSOM; `blob:` is the print sheets,
  `data:` the stamp images. No font is fetched (a run's own font is a
  `FontFace` from bytes) and no frame loads a URL (print is a `srcdoc`).
- **The engine worker** (`/assets/worker-*`, under its own response's
  policy): `default-src 'none'; script-src 'self' 'unsafe-eval'
  'wasm-unsafe-eval'; connect-src 'self'`. It compiles both WASM modules and
  runs pdfium's Emscripten glue as a function body
  (`packages/vitela-core/src/worker/pdfium.ts`); nothing else gets eval.
- **Tests:** `src/features/hosting/headers.test.ts` (the `_headers` reading),
  `csp.test.ts` (both policies, exactly, against the real file) and
  `scripts/hostHeaders.test.ts` (the preview plugin is in the Vite config and
  serves each path what the file gives it). `e2e/csp.spec.ts` checks both
  headers arrive, then opens, renders, highlights, retypes a run in Edit
  content, saves and prints, failing on any `securitypolicyviolation` (page
  or print frame) or console report.
