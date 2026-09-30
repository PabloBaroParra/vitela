# Android shell

This Jetpack Compose baseline opens a PDF with the Storage Access Framework,
retries password-protected opens, reads the document as one continuously
scrolling page list, searches text, and delegates printing of the selected
source PDF to Android's print framework. It uses the byte-oriented `pdf-ffi`
UniFFI API exclusively.

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

Previous/Next and search navigation do not swap a page — they set a scroll
target the list animates to, so the reader keeps one scroll position. The page
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

After an edit everything the shell keeps by page position is re-read
(`viewer/PageLayout.kt`): page count and sizes, rendered bitmaps, search hits,
text selection and annotations. Undo and redo re-read the layout too, but only
once the session has edited its pages. A thumbnail is a small render taken on
demand as its card scrolls into view; a moved page keeps its picture and only a
turned one is rendered again.

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
full rewrite, and a field edit never needs one. As with every page tap on this
shell, the tap is read in the page's unrotated space, so on a turned page a
field lands in the wrong place. That is a known gap.

## Edit content

**Edit content** arms a mode in which a tap on a line of text the page itself
paints opens it in a dialog for retyping, and a tap on an image it paints opens
it for resizing
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

The same read reports the page's images — resource and inline alike — with
pending resizes applied, and each is outlined in green. A tap lands on text
first where the finger is on it (a caption over a photo), then on an image it
is inside, and only then on the nearest text or image within reach. The
**Resize image** dialog takes a width and a height in points; the image keeps
its top-left corner, as a resized form field does, and is stretched to the new
box (`ResizeImage`). Unlike a field it is not kept on the page, since an image
may already hang off it. A size that is not a finite, positive number, or a
refusal from the core, keeps the dialog open with what was typed. A resize is
one undoable entry, redraws the page and re-reads the outline; the gate is the
same `content_editing_allowed`. Moving, replacing and deleting images are still
Linux-only.

## Native prerequisite

PDFium is an external runtime prerequisite. This repository does **not** vendor,
download, or claim to distribute PDFium Android binaries. Obtain compatible
non-V8 `libpdfium.so` files yourself, with their required license notices, for
each ABI the app packages. The required release must match the `pdfium_7763`
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

It builds `pdf-ffi` with `cargo-ndk`, copies each externally supplied PDFium
library into `app/src/main/jniLibs/<abi>/`, and generates matching Kotlin
bindings from that exact `libpdf_ffi.so`. The copied libraries and generated
bindings are local build artifacts, ignored by Git.

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
