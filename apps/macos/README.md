# Vitela macOS development shell

This SwiftUI shell opens local PDFs through the byte-based `pdf-ffi` UniFFI
contract, presents ordered page slots in one vertical scroll view, and clamps
zoom to `0.5...4.0`. It is a development artifact only: it carries an ad-hoc
signature so it can run on Apple Silicon, but it is not notarized, installable,
or approved for distribution.

**Minimum macOS is 12.0**, not 11.0: the pinned PDFium 7763 build declares
`minos 12.0`, and Vitela cannot claim a floor its own bundled renderer does
not meet. Lowering it again means pinning a different PDFium than the one
Linux, Windows and Android share.

## Layout

Per the repository's "no monolithic shells" rule, the target is split by
responsibility:

- `Vitela/VitelaApp.swift` — entry point only, nothing else.
- `Vitela/Views/AppRail.swift`, `Vitela/ViewerViewModel+ShellNavigation.swift`
  — persistent branded Home, Recent, My files and tool navigation, in the
  same order as Linux's `shell.rs` and Windows' `AppRail`. At 1000pt and
  above it is 176pt wide, like Windows; narrower windows use a 56pt icon
  column with the same accessibility labels and tooltips. Recent reveals
  its heading and focuses the newest visible card. The model owns navigation
  and derives the active rail item from the workspace and selected tools tab,
  never a second copy: like Windows, Comments selects no rail item, and Home
  or Organize returns restore the selected tab's item.
  Home hides editor controls and panels without closing or changing the
  document; Return to Document restores its mode and page. File picking
  and protection use the existing native flows. Sign reveals Fill & Sign.
- `Vitela/AppDelegate.swift` — owns the view model; asks about unsaved
  changes before quitting, and passes on files Finder opens with Vitela.
- `Vitela/VitelaCommands.swift` — the menu bar (New, Open, Open Recent, Save As, Protect,
  Compress, Extract Pages, Split PDF, Export Pages as Images, Print, Share, Undo,
  Redo, the Go menu's page and annotation navigation, and View ▸ Pages and
  zoom).
- `Vitela/ViewerViewModel.swift` — file selection, File ▸ New, the render
  queue, and opening the bundled sample documents. Pages render at the
  screen's backing scale, so a Retina display gets a 2× bitmap, inside
  Linux's 24–600 DPI and 8-megapixel bounds
  (`../apple/Shared/ViewerStore+RenderDpi.swift`). Past that budget the
  part of a page in view is drawn again from 1024-pixel tiles at the full
  DPI (`ViewerViewModel+Tiles.swift`, `Views/PageTilesView.swift`,
  `../apple/Shared/ViewerStore+Tiles.swift`) — Linux's `tile_plan`.
- `Vitela/ViewerViewModel+Save.swift` — Save As (signature warning, atomic
  write) and the unsaved-changes guard.
- `Vitela/ViewerViewModel+ContentEdit.swift`, `Views/ContentTextSheet.swift`,
  `Views/ContentOutlineOverlay.swift` — Edit Content: outlines of the lines,
  images and vector paths a page paints. Text is outlined and edited by
  block (B25 T-207): runs the core reads as one piece of text open, move,
  retype and delete together as one undo step (`ContentTextBlock`), and a
  run that joins nothing keeps the single-run edits. A click opens a block
  to retype or delete, a drag moves a block or an image, a corner resizes an image,
  Replace Image… and Add Image… swap or place a PNG or JPEG. A line or a
  box's border (B25 T-206) is selected by a click, moved by a drag, reshaped
  by a corner and removed by Delete Shape or ⌫; text inside a frame keeps
  its click, and the path stays selected for the next edit without a save.
  A press on empty page drags a marquee that gathers every block, image and
  shape it touches; Shift-click adds or removes one item, a drag inside the
  dashed box moves them all and Delete Selection or ⌫ removes them all,
  each one undo step (B25 T-208, `+ContentSelection.swift`). ⌘C and ⌘X
  copy or cut what is selected, and ⌘V pastes it with its top-left corner
  where the pointer last was, on any page or in another document
  (T-209, `ViewerViewModel+ContentClipboard.swift`,
  `../apple/Shared/ViewerStore+ContentClipboard.swift`); outside Edit
  Content, or with nothing held, ⌘V still pastes an image stamp.
  All run as page work (`../apple/Shared/ViewerStore+ContentEdit.swift`,
  `+ContentImages.swift`, `+ContentPaths.swift`).
- `Vitela/ViewerViewModel+Watermark.swift`, `Views/WatermarkSheet.swift` —
  Add Watermark…: text or a PNG/JPEG picture, pages, size, opacity, angle
  and over/behind the content, each kind starting from the core's look; the
  sheet stays open until the core has answered, so a character Helvetica
  cannot show, or a file that is not a picture, is refused under the fields. The rules and
  the recording are `../apple/Shared/ViewerStore+Watermark.swift`, one
  content edit through the page-work pipeline.
- `Vitela/ViewerViewModel+TokenSigning.swift` — File ▸ Sign with Card or
  Token…: a PKCS#11 module and its PIN (empty for the token's own prompt),
  then the certificate-file signing path.
- `Vitela/KeychainSigningStore.swift` — File ▸ Sign with Keychain
  Identity…: the RSA and P-256 identities in the user's keychains, handed to
  the core as a `NativeSigningStore`; keys never leave the Keychain.
- `Vitela/ViewerViewModel+Recents.swift`, `RecentDocumentStore.swift`,
  `Views/HomeView.swift` — the start screen's Recent list (by day, page-1
  previews, search, Remove from Recent or Delete on a focused card) and File ▸ Open Recent. The rules
  are `../apple/Shared/RecentDocuments.swift`; the app keeps the list in
  Application Support, a view model made without that store (every test)
  keeps it in memory.
- `Vitela/AppVersion.swift` — Home's footer, "Vitela <version>" from the
  `VITELA_VERSION` the build was handed (`Info.plist`'s `VitelaVersion`), or
  "Vitela dev" — Linux's `home/version.rs`.
- `Vitela/ViewerViewModel+HomeTools.swift`, `Views/HomeToolsColumn.swift` —
  the start screen's Tools grid (Edit, Annotate, Sign, Organize, Compress,
  Protect: with no document open a tile opens the chooser first and goes
  there once the file loads), Quick actions and the keyboard reference —
  Linux's `home/tools.rs`. The Home rail item and Home ▸ Return to Document
  (⇧⌘H) switch between Home and the open document without changing it.
- `Vitela/ViewerViewModel+Share.swift` — File ▸ Share…: signature
  question, the bytes a save would write in a fresh temporary folder, the
  share picker, and sweeping copies older than an hour.
- `Vitela/ViewerViewModel+Print.swift` — Print… (⌘P): the standard print
  panel over `Views/PrintPagesView`, which paginates one paper page per PDF
  page and renders each on demand (144 DPI for the panel's preview, 300 DPI
  for the printer), scaled to fit the printable area.
- `Vitela/ViewerViewModel+Navigation.swift` — the Go to page prompt and
  the Pages column (`Views/PagesSidebar`), which shares Organize's thumbnails.
- `Vitela/Views/SearchNavigationBar.swift` — search controls and page
  navigation in adaptive groups, like Linux's `FlowBox` and Windows'
  `ToolbarPanel`: two rows when narrow, a full page readout and search
  feedback wrapping below rather than pushing navigation off-screen. Query
  and keyboard focus remain owned by the root, including Home's ⌘F return.
- `Vitela/ViewerViewModel+Zoom.swift` — feeds the page list's size to the
  fit-width / fit-page zoom modes, waiting for a live resize to settle.
- `Vitela/ViewerViewModel+FileDrop.swift` — PDFs dropped on the window or
  sent by Finder (declared in `Vitela/Info.plist`).
- `Vitela/ViewerViewModel+PasteImage.swift` — ⌘V outside a text field
  stamps the clipboard's bitmap (never its text) at the centre of the page
  in view; a PNG or JPEG dropped on a page is stamped where it lands.
- `Vitela/ViewerViewModel+Extract.swift` — File ▸ Extract Pages…: range
  sheet, destination, atomic write.
- `Vitela/ViewerViewModel+Split.swift` — File ▸ Split PDF…: cut points,
  folder, one replace question, parts written one at a time.
- `Vitela/ViewerViewModel+ImageExport.swift` — File ▸ Export Pages as
  Images…: choices, folder, one page at a time, never overwriting.
- `Vitela/ViewerViewModel+Compress.swift` — File ▸ Compress…: preset,
  signature question, and a destination only when something smaller came out.
- `Vitela/ViewerViewModel+Protect.swift` — File ▸ Protect with Password…:
  two passwords, signature question, atomic write, reopen.
- `Vitela/ViewerViewModel+Pages.swift` — the Organize mode and running
  page edits on the queue.
- `Vitela/Views/OrganizeHeader.swift` — Organize's wrapping document-wide
  actions, including Save As and history, like Linux's `organize/header.rs`.
  Organize gets its own workspace, not the reader's tools and duplicate
  Pages sidebar; the panels' state is kept for returning. ⌘F leaves Organize
  for the reader before focusing search, like Windows' `FindDocument_Invoked`.
- `Vitela/ViewerViewModel+Import.swift` — Organize ▸ Add PDFs: picking,
  per-source passwords and warnings, progress and Cancel import while the
  picks are prepared, then one undoable import.
- `Vitela/ViewerViewModel+OrganizeDocuments.swift`,
  `Views/OrganizeDocumentsList.swift` — Organize's Documents view, where it
  opens: one card per run of pages from the same PDF (named by its file),
  moved by its buttons or by a drag to a gap, turned or deleted whole. The
  grouping is the core's `document_blocks`
  (`../apple/Shared/ViewerStore+Blocks.swift`).
- `Views/OrganizeAutoscroll.swift` — scrolls either Organize view while a
  card is dragged near its top or bottom edge, with Linux's
  `organize/autoscroll.rs` ramp.
- `Views/ColorSettler.swift` — holds the color panel's picks until they
  settle, so one annotation color change is one undo step (Windows pins
  this in `AnnotationStyleParityTests`).
- `Views/HomeDropZone.swift` — Home's "Start a new task" area: drop a PDF
  on it or click it to choose one (Linux's `home/hero.rs`, Windows'
  `HomeDropZone`); the drop goes through `ViewerViewModel+FileDrop.swift`.
- `Views/ShellIconImage.swift` — draws a `../apple/Shared/ShellIcon.swift`
  drawing from the shared `assets/icons` set (bundled as `Resources/icons`)
  as a template image tinted like the control's text — Linux's `icons.rs`,
  Windows' `Viewer/ShellIcons.cs`. The toolbars, page navigation, search,
  Home's tiles and quick actions and Organize's cards use them.
- `Views/ShellPaletteColor.swift` — Vitela's palette roles
  (`../apple/Shared/ShellPalette.swift`: the accent, the canvas behind the
  pages, Protect's tile tint) as colours that follow Light and Dark Mode —
  Linux's `theme.rs`, Windows' `Themes/Palette.xaml`. The shell's chrome
  (drop targets, the current page and search row, tinted controls) wears
  Vitela's violet instead of the system accent; window chrome stays native.
- `Views/SideColumn.swift` — the draggable divider beside Pages, search
  results and the Tools panel: drag it or press
  ← / → to resize, past the minimum to fold (Windows' `SideColumn`, Linux's
  `Paned`s).
- `Vitela/ViewerViewModel+ToolsSidebar.swift`, `Views/ToolsSidebar.swift`,
  `Views/ContentEditPanel.swift` — one resizable tools column with Edit,
  Annotate, Comments and Fill & Sign tabs, in Linux/Windows order, opening on
  Annotate. The tab strip wraps even at the 220pt minimum. The model owns
  selection and visibility; changing a tab never changes a document mode or
  records an edit. Edit/Annotate rail and Home destinations reveal their tab.
  Inactive tab views stay mounted but disabled and hidden from accessibility,
  preserving unsubmitted field-inspector drafts without saving them.
  Properties lives under Annotate; existing form controls and `SigningPanel`
  share one scrolling Fill & Sign page. Draw Signature places a visual mark;
  Certificate File, Keychain Identity and Card or Token start the existing
  digital-signing flows, with their existing guards and native prompts.
  Signing refusals are shown in the panel. Sign from the rail or Home reveals
  these choices without opening a certificate chooser or editing the document.
  Folding, Home, Organize and Find preserve the selection
  and last width; Escape still disarms an annotation when its tab is hidden.
  The File menu retains direct access to all three digital-signing sources.
- `Vitela/ViewerViewModel+Annotations.swift` — routes a drag on a page to
  the armed annotation tool, or else to text selection.
- `Vitela/ViewerViewModel+Signature.swift`, `SignatureStore.swift`,
  `SignaturePng.swift` — Draw Signature: the remembered signature, the pad's
  strokes as a PNG, and arming the stamp with it.
- `Vitela/Views/GeometrySheet.swift` — Position… and Size… for the
  selected annotation or page image: exact X/Y or width/height in points,
  over the same core move and resize a drag records
  (`../apple/Shared/ViewerStore+AnnotationGeometry.swift`,
  `+ContentGeometry.swift`); a line's X/Y is typed in its Edit text sheet.
- `Vitela/Views/NoteTooltipLayer.swift` — a note's text as a native
  tooltip while the pointer rests on it, without taking any click.
- `Vitela/Views/SearchResultsPanel.swift` — every match of the search,
  listed by page; a click makes one current.
- `Vitela/Views/ViewerToolbar.swift` — the window's top row of actions,
  panel and mode toggles, and zoom. Editing controls now live in Tools.
  Related commands occupy adaptive grid
  cells, wrapping on narrow windows rather than pushing controls off-screen,
  like Linux's `FlowBox` and Windows' `ToolbarPanel`. `AnnotationToolbar`
  likewise wraps its tools and selected-annotation controls. Both use grids
  available on macOS 12, not macOS 13's `Layout` protocol.
- `Vitela/Views/` — one file per view (including the Properties panel), plus
  the `RenderedPage → NSImage` bridge.
- `../apple/Shared/` — the model layer shared with iOS: `PdfCore.swift`
  and its siblings (protocol, value types, failures), `UniFfiPdfCoreClient*.swift` (the only files
  that touch the generated UniFFI API), and `ViewerStore*.swift` (viewer,
  search, selection, editing, print, page-navigation, zoom, page-structure,
  thumbnail and annotation state, including stepping the selection through
  annotations and revealing the one selected).

`ViewerStore` is main-thread only, with one documented exception:
`renderResult(for:)` runs on the render queue and touches nothing but the
immutable client and the request handed to it.

## Rendering and zoom

Each `PageSlot` records the zoom its current bitmap was rendered at. Changing
zoom makes visible slots stale, so they re-render at `72 * zoom` DPI instead of
upscaling the old bitmap; the previous bitmap stays on screen meanwhile. A slot
already current at the active zoom is never re-rendered when it scrolls back
into view, and a failed page waits for an explicit Retry rather than restarting
the same doomed work on every appearance.

Documents open in **fit width**, as on Linux and Windows. Fit width and fit
page resolve each page against the page list's size on its own
(`ViewerStore.zoom(forPage:)`), so a landscape page in a portrait document
fits too; resizing the window re-fits them once the resize settles. Zoom
in/out steps through Windows' ladder from the factor shown and switches to
an explicit zoom, which no resize moves.

## Page edits

A page edit (Organize ▸ rotate) goes through the core's edit log like any
other edit, but it changes what is drawn: the render side is re-derived from
the pending edits (`refresh_preview`), the page sizes re-read, and every
page slot rebuilt under a new generation, so no render, search hit or text
selection of the old layout survives. Once a session has edited its pages,
every undo and redo re-derives the layout the same way. The core work runs
on the serial queue, behind any render of the old layout.

## Annotations

Annotations the file already carried are drawn by the renderer. The ones
this session adds are listed by the core and drawn by `AnnotationOverlay`
until a save writes them into the file, the same split Linux and Windows
use. Overlay positions come from the core's turn-aware `place_rect` /
`point_to_pdf`, so a rotated page gets them in the right place.

Print and Export Pages as Images have no overlay pass, so an annotated
session renders them from the core's output snapshot instead — the
document as a save would write it, reopened under the session's password
(`ViewerStore+OutputSnapshot.swift`). Without annotations they render the
open document itself.

## Building

`scripts/build-macos.sh --assemble` runs the whole chain: `cargo build
--release`, rewriting the cdylib's install name to `@rpath/libpdf_ffi.dylib`,
regenerating the Swift bindings, then `xcodebuild`. The Xcode target links
`-lpdf_ffi` out of `target/release` and finds `pdf_ffiFFI.h` through
`Generated/module.modulemap`; a build phase copies both dylibs into
`Contents/Frameworks`, so `xcodebuild test` also produces a test host that can
launch.

`ARCHS` is pinned to `$(NATIVE_ARCH_ACTUAL)`, because cargo builds
`libpdf_ffi.dylib` for the host triple only — a universal app would fail to
link its second slice. Shipping a universal app means `lipo`-ing the Rust
dylib first, and belongs with distribution signing in T-059.

```sh
export PDFIUM_DYLIB=/absolute/path/to/libpdfium.dylib
bash scripts/build-macos.sh --assemble
bash scripts/build-macos.sh --verify-bundle build/Vitela.app
```

## macOS CI validation

The `macos.yml` workflow downloads the pinned PDFium 7763 universal build,
generates the Swift bindings (failing if `pdf_ffi.swift`, `pdf_ffiFFI.h`, or
`pdf_ffiFFI.modulemap` is missing), runs Rust and Swift tests, and assembles
`build/Vitela.app`. It then fails closed unless the bundle has an executable,
an `Info.plist`, both dylibs, and every bundled Mach-O slice declares
`minos <= 12.0`.

The job publishes nothing. The bundle exists only inside the run, as proof that
the shell assembles and clears the gate — uploading it is held back until
T-059 adds distribution signing and notarization, so no unnotarized build can
be handed to anyone by accident.

`xcodebuild` is driven through the shared scheme at
`Vitela.xcodeproj/xcshareddata/xcschemes/Vitela.xcscheme`, so CI runs the same
build and test actions a local Xcode does.

`apps/macos/Generated/`, `apps/macos/DerivedData/`, and `build/Vitela.app/`
are generated outputs and can be removed to roll back the native shell without
affecting Rust core behavior or other platforms. A representative multi-page
PDF must still be opened, scrolled, and zoomed on physical Apple hardware
before any distribution decision.
