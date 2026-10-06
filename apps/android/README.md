# Android shell

This Jetpack Compose baseline opens a PDF with the Storage Access Framework,
retries password-protected opens, reads the document as one continuously
scrolling page list, searches text, and prints through Android's print
framework (see [Printing](#printing)). It uses the byte-oriented `pdf-ffi`
UniFFI API exclusively.

## Printing

`PdfPrintDocumentAdapter` rasterizes the pages the print dialog asks for, at a
fixed 300 DPI (as GTK and Windows do), onto a `PrintedPdfDocument` and hands
that to the spooler; it honours the requested page range and reports the real
page count. Each page is scaled to the sheet's content area, aspect preserved
and centred. A page the core cannot render (over its raster ceiling) is left
blank rather than failing the job.

The pages are not rendered from the document on screen: its render preview
leaves the session's annotations out (the shell overlays them), so a print
would lose unsaved ones. `ViewerViewModel.printDocument()` instead asks the
core for its output snapshot (`output_snapshot`: every applied edit and
annotation, saved and reopened as a throwaway document), which the adapter owns
and closes in `onFinish`. The core reopens it under the password it already
holds, so a password-protected document prints too, without the shell keeping
a password; only rasterized pages reach the spooler, never encrypted bytes.
The range and fit-and-centre maths live in
[`print/PrintPages.kt`](app/src/main/kotlin/dev/vitela/pdf/print/PrintPages.kt),
free of `android.*` so the JVM tests cover them.

## Continuous reader

Every page of the document is a slot in a single `LazyColumn`
([`viewer/PageList.kt`](app/src/main/kotlin/dev/vitela/pdf/viewer/PageList.kt)),
but only a window around the viewport is ever rasterized. Two properties make
that safe on a phone:

- **Slots are laid out before they are rendered.** `PdfDocument.pageSizes`
  reports every page's media box up front, so an unrendered slot already
  occupies its correct height. Without it the list would resize under the
  user's thumb each time a render landed, and scrolling would fight itself.
- **The resident bitmap set is bounded.** A full-width page is several MB as
  ARGB_8888, so decoding a whole document is an OOM, not a slow path.
  `PREFETCH_PAGES` decides what is rasterized ahead of the scroll and
  `CACHE_PAGES` decides what survives once it scrolls off — at most
  `visible + 2 * CACHE_PAGES` pages are held at once
  ([`viewer/PageWindow.kt`](app/src/main/kotlin/dev/vitela/pdf/viewer/PageWindow.kt)).
  Raising either constant multiplies peak heap by the page size.

`ViewerViewModel.onVisibleRangeChanged` is the only thing that triggers a
render, which is what keeps that bound honest; a render that finishes after
its page scrolled out of the cache window is dropped rather than cached. This
mirrors the GTK4 shell's viewport tick (`apps/linux-gtk/src/app/render.rs`).

Previous/Next, search navigation and the page list do not swap a page — they
set a scroll target the list animates to, so the reader keeps one scroll
position. The page list opens from the page counter ("Page 2 of 5"): picking a
numbered row jumps there, like the Linux and Windows page lists. Above the list,
a page number can be typed (parity with Windows Go to page): the field opens
with the current page selected, and Go stays disabled unless the text is a
whole number in 1..N — a number past the end is a typo, not a request for the
last page, so it is refused rather than clamped. First page and Last page
(parity with Windows) jump to either end; each is disabled when the counter is
already on that end, so both are disabled for a single page. Previous page and
Next page sit on either side of the page counter (parity with Windows) and step
one page from it; they stop at the ends rather than wrap. All of it is disabled
while the Organize grid stands in for the reader or a document is loading, since
a target set then would fire later over the page Organize returns to. The page
counter reports the page covering most of the viewport, not the first one
visible: at the bottom of a document the previous page keeps a sliver on
screen, so "first visible" could never say "3 of 3".

## Fit-to-width and button zoom

Pages rasterize at the density that makes them exactly fill their slot —
`renderDpi(pageSize, viewportWidthPx, zoomFactor)` — instead of a fixed DPI, so
a page is sharp on a phone without being wastefully oversized on a small screen.

Two consequences the code has to handle, and both are easy to get wrong:

- **Density must not scale peak heap with the display.** A Letter page across
  a phone's 1080 px is ~1.5 Mpx (6 MB); unfolded across 2560 px it is ~8.5 Mpx
  (34 MB), and `CACHE_PAGES` of those at once is an OOM on a device that is
  not otherwise short of memory. `MAX_PAGE_PIXELS` caps the raster at ~16 MB
  per page, trading a little sharpness on very wide viewports for a bound that
  holds on every device. A degenerate MediaBox is separately clamped by
  `MAX_RENDER_DPI`, mirroring the GTK shell.
- **A layout change keeps a bounded visual bridge.** Each bitmap is tied to the
  width and zoom it was fit to, so a layout change starts a new generation and
  invalidates every render already in flight. Cached pages in the existing
  cache window remain underneath the sharp replacement, avoiding a blank or
  spinner frame; they are removed when replaced or evicted with the same
  window. `ViewerViewModel.layoutGeneration` is how a finished render knows it
  is answering a question nobody is asking any more.

The reader starts at 100% fit-to-width. Its accessible **Zoom out** and
**Zoom in** controls move through this bounded custom scale: 10%, 25%, 50%,
75%, 100%, 125%, 150%, 200%, 300%, 400%, 600%, and 800%. The visible percentage
reports the active level. Zoom changes page-slot geometry and starts sharp
 replacements at the scaled DPI, retaining the previous full-page bitmap only
 as that temporary bridge. It does not use `graphicsLayer` bitmap scaling or
 region tiles. Pages wider than the viewport use one horizontal scroll position
 for the continuous reader.

Pinch gestures and fit-page remain out of scope — see T-084.

## Text selection

In the Select tool, **long-press and drag** selects text; a tap clears it. A
plain drag is still the page list's scroll: it passes touch slop before the
long-press fires, which cancels the selection gesture. **Copy** puts the
selected text on the clipboard. With text selected, choosing Highlight,
Underline, or Strikeout marks the selected lines.

The shell does no text geometry of its own. `PdfDocument.pageCharacters`
wraps `pdf-ffi`'s `FfiPageCharacters`, so the caret under the finger, the
line rects to paint, and the copied text all come from
`pdf_render::selection`, the code the GTK and Windows shells also use.
An earlier build intersected character boxes with the drag rectangle in Kotlin.
That selected a box instead of following reading order, and it could not
produce text to copy.

The characters load off the main thread, so the finger is usually moving
before they arrive. `TextSelectionDrag` stores the anchor and focus as points
and turns them into carets only once the characters are attached. That way a
move made during the load is not lost. The drag owns the native handle. It
releases it when the finger lifts, when another drag starts, or when the
document is replaced. If characters arrive after the drag has closed, they are
released straight away.

A selection covers one page. Long-press without dragging selects nothing,
because the core has no word-boundary query yet. If the document's
permissions forbid text extraction, the reader says so in the status line.

## Annotation editing and save copies

When a document permits annotation editing, the reader exposes highlight,
underline, strikeout, ink, shape, text-note, and image-stamp tools. Selecting
an existing annotation enables move, resize, delete, and supported color
restyling actions. A tap selects or places; an editing drag moves, resizes, or
draws, while an unselected pointer drag remains available to scroll the page
list. Image stamps are picked through SAF and use the core's placement policy,
so Android does not invent its own aspect-ratio or anchor rules.

**Note** asks for text after the tap or drag chooses its rectangle, like the
Windows shell. Notes accept multiple lines and the text is kept exactly as
typed; **Add** stays disabled while it is blank. Cancel closes the prompt
without creating an annotation or an undo step. Adding records one undoable
edit through the core; save to keep the note. The prompt carries the document
it was opened for, so an answer can never land in a document that replaced it.

Select a note with a tap or with **Previous annotation** / **Next annotation**,
then choose **Read note** to see its text in a read-only, multi-line dialog,
like the Windows shell. Reading is not an edit: it works when annotation
editing is forbidden and changes neither the PDF, its undo history, nor the
dirty state. The text is shown exactly as the core holds it, including a blank
note. Like Windows, only notes in the core's session snapshot can be selected:
annotations already in a reopened file are kept but not yet listed by it.

**Resize** sets the selected annotation's width and height in PDF points, like
the Windows shell, beyond corner dragging and the fixed **Grow** step. The
bottom-left PDF-space origin stays put; dimensions must be finite and positive,
and stamps stretch to the new rectangle. Ink has no rectangle and cannot be
resized. The edit needs annotation-edit permission and records one undo step
through the core's resize command. Cancel or unchanged dimensions leave history
alone, redo included. The dialog keeps a snapshot of the rectangle it opened
on and of its document: if an undo changed the annotation meanwhile, or the
document was replaced, nothing is resized.

**Move to** sets the selected annotation's bottom-left X/Y in unrotated PDF
points (X increases rightward, Y upward), keeping its size, like the Windows
shell. Ink uses the bottom-left of its points' bounds and moves every point by
the same offset. Coordinates must be finite; zero and negative values are
allowed. The edit needs annotation-edit permission and records one undo step
through the core's move command. Cancel or an unchanged position leave history
alone, redo included. As with **Resize**, the dialog keeps a snapshot of the
annotation and its document, so nothing moves if either changed meanwhile.

All annotation mutations, undo/redo, byte snapshots, and document replacement
are serialized by `ViewerViewModel`. **Save** writes a complete annotated
snapshot back over the file that was opened; **Save copy** writes one through
SAF's create-document flow instead. The dirty state is cleared only if the
write reports success for the same document revision; an older save cannot
clear edits made while it was being written. Opening another document while the
current one is dirty requires confirmation before replacement.

**Save** is offered only when the provider advertises
`FLAG_SUPPORTS_WRITE` and the persistable write grant was taken. The packaged
sample has no writable origin, so it only offers Save copy. The target is
captured together with the snapshot under the document lane, so a replacement
cannot redirect one document's bytes into another's file. If a provider still
refuses the write, Save is disabled for that document and Save copy remains.
Writes use mode `"wt"`: some providers do not truncate on `"w"`, which would
leave the old file's tail behind a shorter save. SAF offers no atomic rename,
so an interrupted write-back can leave the original incomplete — the same
trade-off as any in-place save through a content provider.

SAF code (reading, permission grants, writes) lives in
`document/SafDocuments.kt`; the ViewModel only sees bytes and an opaque
save-target string.

**Open sample** loads the shared sample document instead of going through the
picker. The file is not stored in this module: `app/build.gradle.kts` adds the
repository's `assets/sample/` directory as an asset source, so the APK ships
the byte-identical file the Windows and Linux shells package (see
[`assets/README.md`](../../assets/README.md)). It still needs PDFium to render,
exactly like a picked file.

## Document properties

**Properties** opens the six text keys of the PDF's `/Info` dictionary (title,
author, subject, keywords, creator, producer). **Apply** queues one undoable
change in the same edit log as the annotations, so Undo reverts it and Save
persists it. An emptied field removes its key rather than writing an empty
string, and an apply that changes nothing queues nothing. The creation and
modification dates are not editable; the adapter carries the file's own values
through every write.

The core does not gate `SetDocumentInfo` itself, so the adapter asks
`content_editing_allowed` — the same permission the Windows shell asks. A
document that withholds it opens the dialog read-only with the reason.

## Export images

**Export images** writes pages as PNG or JPEG files into a folder. The dialog
asks for the pages (all, the current one, or a typed range such as `1-3,7`),
the format, and a resolution of 72-400 DPI (150 by default). Every choice is
checked while the dialog is still open, so a bad range or an oversized page is
corrected where it was typed; only then does the shell ask for a folder
(`OpenDocumentTree`).

Every rule is the core's, not Kotlin's: the range grammar
(`parse_page_selection`), the file names (`page_image_file_name`), the raster
ceiling (`first_page_too_large_to_export`) and the encoding
(`export_page_image`). The ceiling is why the dialog stops at 400 DPI: US
Letter at 600 DPI is over the core's 32 Mpx limit, so a higher notch would
be a control that lies. The same permission as text extraction (`/P` bit 5,
`text_extraction_allowed`) gates the feature: a document that withholds it
opens the dialog refused with the reason.

The export stops at the first failure and reports how many files were written.
A file whose write failed is deleted rather than left truncated. Name
collisions are the provider's to resolve — it renames the new document instead
of overwriting an earlier export. Each page takes the document lane on its own,
so editing is not frozen for the length of a long export.

## Organize pages

**Organize** swaps the reader for a grid of page thumbnails
([`viewer/OrganizeGrid.kt`](app/src/main/kotlin/dev/vitela/pdf/viewer/OrganizeGrid.kt)).
Each card moves its page one step earlier or later, turns it a quarter, or
deletes it, and a "+" inserts a blank A4 page (portrait or landscape) before
it; a trailing "Add page" card appends one. Every change is one undoable entry
in the shared edit log, so the
reader's Undo and Redo keep working, and the document becomes dirty like any
other edit. Pages move by buttons rather than by dragging: a drag inside a
scrolling grid competes with the scroll, and each button is a labelled target.

The core owns whether the document allows it (the assembly permission bit, and
whether the file survives the full rewrite a reorder forces); a refusal comes
back as the sentence to show, as on Windows. The one refusal the shell makes
itself is deleting the last page.

**Add PDFs** appends every page of each picked file
([`viewer/PageImporting.kt`](app/src/main/kotlin/dev/vitela/pdf/viewer/PageImporting.kt))
in the core's two phases, as on Linux and Windows: each file is first prepared
on its own (`prepare_import`) without touching the document — a locked file
asks for its own password, a refused one names itself — and then the whole
pick goes in after the last page as one undo step (`import_prepared`). What
the pages would not bring across exactly (a renamed form field, a link out of
the imported pages) is asked before anything is added, with Cancel as the
prominent answer. It is all or nothing: a cancel or a refusal adds no page,
and every source prepared so far is closed, which frees its decrypted copy in
the core; so is leaving Organize or replacing the document mid-pick.

A **Pages | Documents** selector above the grid switches to one card per
document instead
([`viewer/OrganizeDocumentList.kt`](app/src/main/kotlin/dev/vitela/pdf/viewer/OrganizeDocumentList.kt)):
each contiguous run of pages from the same PDF — the opened file, a PDF added
with **Add PDFs**, or inserted blank pages — named after its file, with
"Part N" when that file is split across runs. A card moves its whole block past
the previous or next one, turns every page of it a quarter, or deletes it; each
is one undoable step (`MovePages`, `RotatePages`, `RemovePages`). The blocks are
the core's (`document_blocks`, the same `derive_blocks` the Linux Documents
view reads) and are asked again after every change, since a move can merge two
runs or split one. Only the names are the shell's: the core reports an added
PDF by the `source_id` its import returned.

After an edit everything the shell keeps by page position is re-read
(`viewer/PageLayout.kt`): page count and sizes, rendered bitmaps, search hits,
text selection, annotations and, while it shows, the document blocks. Undo and redo re-read the layout too, but only
once the session has edited its pages. A thumbnail is a small render taken on
demand as its card scrolls into view; a moved page keeps its picture and only a
turned one is rendered again.

## Rotated pages

A page carrying `/Rotate` — from the file, or a quarter turn in Organize — is
drawn turned, but nothing on it moved: text runs, images, form fields,
annotations, search hits and selection rects all stay in the page's unrotated
space. `PageSize` therefore carries the turn with the drawn size, and every
tap and every overlay on a page goes through
[`viewer/PagePlacement.kt`](app/src/main/kotlin/dev/vitela/pdf/viewer/PagePlacement.kt).
A bare y-flip, which the reader used until then, put every outline and every
tap on a turned page in the wrong place.

`PagePlacement` is a port of `pdf_render::selection`'s `place_rect`,
`place_point` and `point_to_pdf`, which `pdf-ffi` also exports so that shells
need no copy of them, and which Windows calls. Android keeps a copy anyway.
These functions run for every outline on every Compose frame, where a JNA
crossing per rect is a cost the phone pays, and a JVM unit test cannot load the
native library at all. `PagePlacementTest` pins the copy to the Rust tests'
values, case for case. If a Rust case changes, the Kotlin case has to change
with it.

## Form fields

**Form fields** opens a panel below the reader
([`viewer/FormFieldsPanel.kt`](app/src/main/kotlin/dev/vitela/pdf/viewer/FormFieldsPanel.kt))
with one row per AcroForm field. A row fills its field in, and each fill is one
undoable entry in the shared edit log. Filling needs the annotation permission
(`/P` bit 6).

When the document also lets fields be created (`form_field_editing_allowed`),
the panel adds a row of chips (Text field, Checkbox, Radio group, Dropdown)
and a **Move** button on every field. Picking a chip or Move arms the next page
tap, and that tap is the edit. A chip places a new field at the Windows shell's
click size, hanging below and to the right of the tap. Move puts the field's
top-left corner at the tap and keeps its size. A move stays on the field's own
page, because the core's move takes a rectangle and no page. Both keep the
field whole on the page. It is a tap, never a drag, because on a phone a drag
is the reader's scroll. The core names each new field and picks its style and
first options. While a tap is armed it claims every page tap: Edit content, an
armed annotation tool and a text selection are dropped.

Under the same permission each field also shows its width and height in PDF
points. Both commit together when focus leaves the pair (Done lets it go), so
typing a width and then a height is one edit. A resize keeps the field's top-left
corner, the corner a move lands on, so the field grows down and to the right.
It is clamped whole onto the page, and a size that is not a finite, positive
number is refused before it reaches the core. Windows keeps the bottom-left
origin instead; both send the core the same resize command.

Nothing is drawn over the page. Only the renderer paints a field, so every
edit rebuilds the preview and the page redraws. The permission is asked as the
core's own question, not composed from the annotation and content answers:
that composition would also refuse a file whose encryption cannot survive a
full rewrite, and a field edit never needs one. A field is kept whole on its page in the unrotated
space its rect lives in, not at the turned size the page is drawn at. On a
turned page "top-left" and "down and to the right" are the field's own
corners and directions, which the turn has carried elsewhere on screen.

## Edit content

**Edit content** arms a mode in which a tap on a line of text the page itself
paints opens it in a dialog for retyping, moving or deleting, and a tap on an image it
paints opens it for resizing, moving, replacing or deleting
([`viewer/ContentEditing.kt`](app/src/main/kotlin/dev/vitela/pdf/viewer/ContentEditing.kt)).
The run keeps its position and its font; **Retype** queues one undoable entry
in the shared edit log. While armed, the mode claims every page tap — an armed
annotation tool and a text selection are dropped, and choosing a tool leaves
the mode. It is not offered over the Organize grid.

Each page's runs come from the core's `read_page_content` as the page is
shown, and are outlined: solid where the font is kept, dashed where it is a
composite (CID) font the core cannot re-encode, whose retype uses a standard
font instead (`ReplaceTextRunWithInsertedFont`) — the dialog says so before
anything is typed. The core reports pending retypes in that read, so a retyped
run keeps its id and shows its new text, and retyping it again amends the
queued command rather than stacking another.

Only the renderer can paint the new words, so a retype rebuilds the preview
and the page redraws, like a form fill; undo and redo redraw and re-read the
runs. The gate is `content_editing_allowed`, as on Windows. A character the
run's font cannot show is refused by name and the dialog stays open with what
was typed. Unlike the Windows shell, which writes as the reader types, a
retype is committed once, from the dialog.

**Delete** in that dialog takes the run off its page (`RemoveTextRun`) at once,
with no second question, like an image's Delete: it is one undoable entry, and
Undo puts it back. No font is touched, so a composite-font run is deleted as
it stands. The dialog closes before the core answers, so a double tap queues
one delete; a refusal — "This text cannot be deleted." — is reported in the
status line. The delete redraws the page and re-reads it, so the outline goes
with the run. The core checks the delete against the file as last saved, so it
may refuse one on a run with a pending retype; save first. This is content
editing, not a secure redaction: the dialog says so.

**Move** in that dialog closes it and arms the next page tap, like an image's
Move: the run's top-left corner lands on the tap and its text, font and size
are kept (`MoveTextRun`) — the core re-places the original show operands
rather than re-encoding them, so a composite-font run keeps its font too, and
the other runs on the line stay where they were. The armed run's outline is
drawn heavier and **Cancel move** disarms it; one move, of a run or an image,
is armed at a time, and arming an insert drops it. A tap on another page moves
nothing and leaves the move armed; a tap on the corner the run already has
queues nothing ("Text position unchanged."). The move is spent before the core
answers, so a double tap queues one, and an undo or redo disarms a pending
move, since the run may no longer be where it was. Like a delete, it is checked
against the file as last saved, so the core may refuse a run with a pending
retype or move — "This text cannot be moved." — until it is saved; it also refuses a
run painted by the `"` operator, whose spacing a plain move would drop.

The same read reports the page's images — resource and inline alike — with
pending resizes and moves applied, and each is outlined in green; a pending
delete drops it from the read. A tap lands on text
first where the finger is on it (a caption over a photo), then on an image it
is inside, and only then on the nearest text or image within reach. The
**Edit image** dialog takes a width and a height in points; the image keeps
its top-left corner, as a resized form field does, and is stretched to the new
box (`ResizeImage`). Unlike a field it is not kept on the page, since an image
may already hang off it. A size that is not a finite, positive number, or a
refusal from the core, keeps the dialog open with what was typed. A resize is
one undoable entry, redraws the page and re-reads the outline; the gate is the
same `content_editing_allowed`.

**Move** in that dialog closes it and arms the next page tap, the way a form
field is moved: the image's outline thickens, and the tap puts its top-left
corner there at the same size (`MoveImage`). The image stays on its own page —
a tap on another page moves nothing and keeps the move armed — and, like a
resize, it is not kept on the page. The tap spends the move before the core
answers, so a double tap queues one edit; a refusal is reported in the status
line. **Cancel move** next to Done editing disarms it, and an undo or redo
disarms it too, since the image it held may have moved back. A move is one
undoable entry and redraws the page like a resize. The core may refuse a second
geometry edit on the same image while the first is pending; save first.

**Delete** in that dialog takes the image off its page (`RemoveImage`, resource
and inline alike) at once, with no second question: it is one undoable entry,
and Undo puts it back. The dialog closes before the core answers, so a double
tap queues one delete; a refusal is reported in the status line. The delete
redraws the page and re-reads it, so the outline goes with the image; the gate
is the same `content_editing_allowed`. The core checks a delete against the
file as last saved (`validate_content_command`), so it refuses one on an image
with a pending move or resize — "This image cannot be deleted."; save first.

**Replace** in that dialog swaps the image's picture for a PNG or JPEG, keeping
its box: a different aspect ratio stretches to it (`ReplaceImageSource`). The
core first reads the original back through `image_source_bytes`, before any
picker opens, as the Windows shell does: an image with a pending edit, or an
original whose encoding Undo could not restore without loss, is refused there
and reported in the status line, so the reader never picks a file for nothing.
The original is read again when the file lands, so the undo restores what the
page holds then. A dismissed picker or an unreadable file replaces nothing; a
file that is not PNG or JPEG is refused by the core. The pick is spent before
the core answers, so a second one queues nothing, and an undo or redo while the
picker is open drops the replacement, since the image it held may have
changed. A replacement is one undoable entry, redraws the page and keeps the
outline; the gate is the same `content_editing_allowed`.

**Add text** and **Add image** next to Done editing arm the next page tap for
something new, painted as page content rather than as an annotation
([`viewer/ContentInserting.kt`](app/src/main/kotlin/dev/vitela/pdf/viewer/ContentInserting.kt)).
The tap names the new item's top-left corner, as a moved image's — the Windows
shell types coordinates instead, but a finger already points at the page. It
claims the tap even over existing text, is spent by it, and **Cancel insert**
disarms it; arming one drops an armed move or an open dialog. For text the tap
opens a dialog for one line and its size, 1–72 pt, 14 by default; blank text,
a size out of range, or a character Helvetica cannot show keeps the dialog open
with what was typed. **Insert** queues `InsertTextRun` in a Helvetica resource
of its own — a random name the page does not declare, since the core reuses a
font already registered under the name it is given — and the core measures the
line's width itself. For an image, **Add image** first picks a file through
SAF; the tap then places it at the size the core's stamp placement policy
gives it (`stamp_placement`, aspect ratio kept) and queues `InsertImage` under
a random XObject name. A file that is not PNG or JPEG is refused by the
placement, before anything is queued.

Each insert is one undoable entry: it redraws the page and re-reads it, so the
new line or image is outlined like any other. The gate is the same
`content_editing_allowed`. The core checks every content edit against the file
as last saved, where an inserted item does not exist yet, so retyping, moving,
resizing or deleting one before a save is refused; save first, or Undo it.

## Native prerequisite

PDFium is never vendored into this repository. A **distributable** build, the
one Google Play gets, uses the same pinned third-party release as the Windows
and Linux packages. `scripts/fetch-android-pdfium.sh` downloads
`bblanchon/pdfium-binaries` `chromium/7763` for arm64 and x64 and refuses any
archive whose SHA-256 is not the pin. It then checks that each library is build
`7763`, built for Android and the right CPU, and has neither V8 nor XFA. It
also stages the PDFium license notices, plus this project's, as app assets
under `licenses/`.

```sh
eval "$(bash scripts/fetch-android-pdfium.sh | sed 's/^/export /')"
```

That prints the two variables below. You can still supply your own compatible
non-V8 `libpdfium.so` files instead; the release must match the `pdfium_7763`
feature selected by `core/pdf-render/Cargo.toml`.

The packaging script needs the Android NDK (`ANDROID_NDK_HOME`), `cargo-ndk`,
the `aarch64-linux-android` / `x86_64-linux-android` Rust targets, and both
PDFium paths:

```sh
export ANDROID_NDK_HOME=/absolute/path/to/Android/Sdk/ndk/<version>
export PDFIUM_ANDROID_ARM64_V8A=/absolute/path/to/arm64-v8a/libpdfium.so
export PDFIUM_ANDROID_X86_64=/absolute/path/to/x86_64/libpdfium.so
bash scripts/package-android.sh
```

Verify the PDFium build before packaging: its `VERSION` file must report
`BUILD=7763` (matching the `pdfium_7763` feature) and its `args.gn` must have
`pdf_enable_v8 = false`. A mismatched build fails at runtime, not at build
time.

Android distribution APKs must support 16-KB memory pages. Every packaged
`arm64-v8a` and `x86_64` native library needs a `PT_LOAD` alignment of at least
`0x4000`. `package-android.sh` finds `llvm-readelf` from `ANDROID_NDK_HOME` and
fails before Gradle if either generated `libpdf_ffi.so` or supplied
`libpdfium.so` does not meet that requirement. This check also applies to the
actual external PDFium binary, not only its build configuration.

It builds `pdf-ffi` with `cargo-ndk` into `app/build/cargo-ndk/`, copies only
`libpdf_ffi.so` from there and each externally supplied PDFium library into
`app/src/main/jniLibs/<abi>/`, and generates matching Kotlin
bindings from that exact `libpdf_ffi.so`. The copied libraries and generated
bindings are local build artifacts, ignored by Git.

The staging step exists because cargo-ndk's `-o` copies every `cdylib` cargo
reports, dependencies included, and `pdfium-render` declares itself a `cdylib`.
Pointing `-o` straight at `jniLibs` shipped an unused
`libpdfium_render-<hash>.so` in every ABI of the bundle.

The script writes two separate generated trees, and the split matters:
`build/generated/uniffi/kotlin` (registered as a `java.srcDir`) holds the
bindings and the `GeneratedPdfCore.kt` adapter, while
`build/generated/uniffi/resources` (registered as a `resources.srcDir`) holds
only the `META-INF/services/dev.vitela.pdf.core.PdfCoreFactory` descriptor.
A Gradle source directory is compiled but never packaged, so a descriptor
placed under the sources tree never reaches the APK — the app then reports
native support as missing even with every `.so` correctly packaged. Do not run the app without
this step: it will show an explicit native-support packaging error instead of
pretending PDF rendering is available.

Without the native core the app also **disables both open actions** ("Open PDF"
and "Open sample"). `ViewerViewModel.open` returns immediately when no
`PdfCoreFactory` is registered, so leaving the buttons enabled would accept
taps it silently drops — `ViewerState.canOpen` keeps the UI honest about what
it can actually do. This applies to the packaged sample too: it is a real PDF
that still needs PDFium to render.

After packaging, use an installed Android Gradle distribution:

```sh
gradle -p apps/android :app:assembleDebug
```

Before distributing an APK, verify both ELF and APK alignment. The packaging
script performs the ELF verification; its output names every checked library.
Then use the Android SDK Build Tools `zipalign` on the release APK:

```sh
zipalign -c -P 16 -v 4 apps/android/app/build/outputs/apk/release/app-release.apk
```

The command must succeed. Do not use compressed JNI libraries or legacy
packaging modes as a workaround for a library that fails the ELF check.

The focused JVM tests intentionally do not need native libraries:

```sh
gradle -p apps/android :app:testDebugUnitTest
```

## Google Play release

`android.yml`'s `release-bundle` job builds the distributable bundle on every
Android change. It uses the pinned PDFium and the cargo-ndk core, checks the
16-KB alignment, and produces an unsigned `bundleRelease`. It then checks that
the `.aab` declares exactly `arm64-v8a` and `x86_64` and carries `libpdf_ffi.so`,
`libpdfium.so` and the license notices for both.

`abiFilters` limits the app to those two ABIs. JNA and androidx bring 32-bit
libraries of their own. Without the filter, Play would install the app on
32-bit devices that have no native core, where it can open nothing.

A release tag sets the version. `release.yml` translates the tag with
`scripts/release-version.sh`: `semver` gives `versionName` and `android-code`
gives `versionCode`. The `versionCode` folds alpha < beta < rc < final into one
growing integer, for example `v0.2.0-beta.1` → `200101` and `v0.2.0` →
`200300`. `release.yml` passes both values into this build:

```sh
./gradlew :app:bundleRelease -Pvitela.versionCode=200101 -Pvitela.versionName=0.2.0-beta.1
```

Every other build keeps `1` / `0.1.0`.

`android-release.yml` signs that exact bundle with the Play upload key
(`jarsigner`) and uploads it to the track the tag maps to:

| Tag | Play track |
| --- | --- |
| alpha | internal |
| beta, rc | beta |
| final | production |

The one-time Play Console and GitHub setup is listed in that workflow's header.
