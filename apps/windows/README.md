# Windows shell

`Pdf.Windows` is the WinUI 3 shell. Its views use only the handwritten C# facade;
generated UniFFI types are isolated in `Facade/GeneratedPdfCore.cs`.

The view is split by responsibility rather than kept in one file: `MainWindow.xaml.cs`
holds bootstrap and the open path, and one partial per feature carries the rest —
`MainWindow.Viewer.cs`, `MainWindow.Search.cs`, `MainWindow.Print.cs`. Presentation
maths that does not need a UI runtime lives beside them in `Viewer/` — `PageZoom.cs`,
`PageWindow.cs`, `PageRenderPlan.cs`, `ViewportTilePlan.cs` — which is why the facade
suite can cover it.

## Deep zoom

Past the point where a whole page no longer fits the per-page pixel budget, the
viewer stops scaling one bitmap and renders the viewport as tiles.

Three rules make that affordable, and each exists because breaking it was slow:

- **Tiles sit on a fixed grid in the page's pixel space, never on the scroll
  offset.** A grid anchored to the viewport yields a different tile set for every
  scrolled pixel, so nothing rendered can ever be reused.
- **One request covers the viewport.** `render_page_tiles` rasterizes every tile
  in a single actor job with the page loaded once — `FPDF_LoadPage` parses the
  content stream, which on a text-heavy page costs more than the raster itself.
  Tile batches also get their own lane in `PdfDocumentFacade`, so a batch and a
  full-page render never cancel each other.
- **A tiled page still gets a full-page bitmap, as a cheap bridge.** It is only
  ever seen stretched — before tiles land, and wherever one has not — so it is
  budgeted well below the page's own render target (`PageZoom.BridgeDpi`).
  Without it a page keeps whatever the last untiled zoom produced, which after a
  jump up from 10% is the minimum-DPI floor.

## First vertical

The document and editing toolbars wrap onto additional rows when the window
narrows, keeping their controls reachable. Zoom, fit, history and search controls
stay together as groups; keyboard shortcuts and tab order follow the same controls.
Long document titles are truncated, while print status messages wrap.

**Panels** shows or hides the right-hand column to give the document more room.
Fit width and Fit page adjust to the available space; a custom zoom stays fixed.
The column scrolls vertically so expanded properties, form fields and search
results remain reachable in short windows. Hiding it preserves the panel contents
and selections; the visibility choice survives opening another document. Running
a search opens the column again to display its status and results. This is a
window layout choice and does not change the PDF or its undo history.

Drag the divider beside the panels to adjust their width, or focus it with Tab
and press Left to widen or Right to narrow them. The width ranges from 300 to
600 DIPs, leaving at least 240 DIPs for the document when the window has room.
A smaller window temporarily limits the panel width; expanding it restores the
requested width. Hiding the panels or opening another document also preserves
the width for this window.

The current vertical opens a local PDF — either through the file picker or the
**Open sample** button, which loads the shared sample document copied beside
the executable as `Assets\vitela-sample.pdf` (see
[`assets/README.md`](../../assets/README.md)) — lazily renders its pages, and
supports case-sensitive exact-text search. Selecting a result navigates to its page and highlights the
matching PDF-space character geometry. CI runs in
`.github/workflows/windows.yml`: the Rust workspace on a Windows runner, the
facade suite, and a full shell build (native dll + regenerated bindings +
MSBuild).

The current shell also edits document properties and can protect a saved copy
with AES-128 encryption. Protection asks for distinct open and permissions
passwords, warns before invalidating an existing signature, writes through a
temporary destination, and reopens the protected bytes with the new open
password.

Search selects the first result automatically. **Previous match** and **Next
match** move through the results with wraparound, scrolling to the selected
page. All matches are highlighted; the selected match has a stronger accent.
The status shows the selected match number and the submitted query, even if the
search box is edited afterward.
Press Enter in the search box to run the same search as **Find**.
The search box indicates that matching is case-sensitive; while a query runs,
the status names it, and an unsuccessful search reports which query found no matches.

The **Pages** panel lists every page of the open document. Select a page to jump
to it without searching; the selected row follows the page at the top of the
viewport when scrolling or navigating through search results. The list uses the
current zoom and page order, including after organizing or undo/redo. Navigation
does not change the PDF or its history, and is disabled while organizing or busy.

**Go to page** (Ctrl+G) jumps directly to a numbered page, even with the panels
hidden. Enter a whole number from 1 through the document's page count. The dialog
starts at the current viewport page; Cancel keeps the current position. Navigation
uses the current zoom and preserves undo/redo. Opening another document or changing
the page order while the dialog is open invalidates the request.

**Previous page** and **Next page** move one page from the page at the top of
the viewport, using the current zoom even when the panels are hidden. They do
not wrap: Previous is disabled on the first page and Next on the last. Both
are disabled without a visible document, while busy, organizing or showing a
modal dialog. Scrolling updates their availability; navigation preserves PDF
contents and undo/redo.

**Resize image** lists the content images on the page at the top of the viewport.
Choose an image and enter its width and height in PDF points; its origin stays
fixed. Resource images and inline images use the same Rust resize command.
Dimensions must be finite and positive. The edit requires content-edit permission
and a document that can be fully rewritten; it refreshes the preview and supports
undo/redo. Save to keep the change. Reopen the dialog after any intervening edit.

**Move image** uses the same image list on the visible page. Enter X and Y in
PDF points (X increases rightward, Y upward) to move the image without changing
its dimensions. Coordinates must be finite; zero and negative values are allowed.
Moving uses the same permission, preview, undo/redo and snapshot checks as resizing.
The core refuses a second geometry edit on the same image while the first is
pending. Save first, then reopen the dialog; rereading alone does not clear it.

**Delete image** lists the content images on the visible page and removes the
chosen image after confirmation. Resource and inline images use the same Rust
command, with preview refresh and undo/redo. Deletion requires content-edit
permission and a full rewrite. Save first if the image already has a pending edit;
reopen the dialog after any intervening edit. Save to keep the deletion.

**Replace image** lists content images on the visible page, then asks for a PNG
or JPEG. The chosen image keeps its position and dimensions; a different aspect
ratio stretches to that rectangle. Original bytes are recovered through Rust
before the picker opens and again at submission, including on imported pages.
Encodings that cannot round-trip without loss are refused so undo can restore
the source. Save first if the image has a pending edit. Replacement requires
content-edit permission and a full rewrite, refreshes the preview and supports
undo/redo. Reopen after intervening edits, and save to keep the replacement.

**Delete text** lists the text runs on the visible page, with their text and PDF
coordinates. Choose a run and confirm deletion; the PDF preview refreshes and
undo/redo restores or reapplies it. Deleting does not substitute composite fonts.
It requires content-edit permission and a full rewrite. Reopen the dialog after
any intervening edit. Save first if the run already has a pending edit, and save
to keep the deletion. This is content editing,
not secure redaction.

**Move text** lists text runs on the visible page and moves the chosen run to
X/Y coordinates in PDF points (X increases rightward, Y upward). Its text, font
and size remain intact, and other runs stay in place. Coordinates must be finite;
zero and negative values are allowed. The edit refreshes the preview and supports
undo/redo under the same permission and full-rewrite checks as Delete text.
Reopen the dialog after any intervening edit. Save first if the run already has
a pending edit, including retyping or moving, and save to keep the new position.
The core refuses runs painted by the double-quote spacing operator.

**Insert text** adds a nonempty single line of Helvetica text to the visible
page as real page content, not an annotation. Enter X/Y in PDF points relative
to the unrotated page's bottom-left and a size from 1–72 pt (14 by default).
Zero and negative coordinates are allowed. Each insertion uses a fresh font
resource so existing fonts and other pending insertions are preserved. Characters
outside the font's WinAnsi encoding are refused by the core. Insertion refreshes
the preview and supports undo/redo; it requires content-edit permission and a
full rewrite. Reopen the dialog after any intervening edit, and save to keep it.

**Insert image** adds a PNG or JPEG to the visible page as real content, not a
stamp annotation. Enter its top-left X/Y in PDF points relative to the unrotated
page's bottom-left (X increases rightward, Y upward). The shared core preserves
its proportions with a longest side of 144 pt, as on Linux. Coordinates must be
finite; zero and negative values are allowed. Insertion refreshes the preview
and supports undo/redo; it requires content-edit permission and a full rewrite.
Reopen the dialog after any intervening edit, and save to keep the image.

**Export images** writes pages of the open document to separate PNG or JPEG
files in a chosen folder: all pages, the current page, or a typed range such
as `1-3,7`, at 72–400 DPI (150 by default). The range grammar, the file names
and the oversized-page check are the core's (`pdf-ffi`'s `export` module), so
this shell and the GTK one agree on all three. Every choice is checked while
the dialog is still open, and a document whose permissions forbid extraction
is refused there too. Existing files are preserved: name collisions receive a
unique suffix. Export stops at the first failed page and reports how many
were written; the open document remains editable.

**Extract pages** prunes a typed range of pages (the same `"1-3,7"` grammar as
Export images) into a new, self-contained PDF, written through a temporary
destination like Save. Nothing about the open document changes: the prune
runs against a clone, sharing the core's cut (`pdf_document::prune::prune_to`)
with the Linux shell's own Extract chain rather than reimplementing it. Two
gates are checked before the destination picker even opens: the document's
extraction permission (`/P` bit 5, the same bit Export images asks) and
whether it can survive a full rewrite at all — an encrypted document opened
with only one of its two passwords cannot be re-encrypted, so it cannot
produce a new page set either. A signed source is written anyway (extraction
never blocks on a signature the reader is not being asked to break), but the
status line says its signature no longer verifies.

**Split PDF** cuts after selected pages (for example `3,7` produces three
PDFs). The cut grammar, boundaries and file names come from the same Rust core
used by Linux; each part is extracted from the open document without changing
it. Parts are written through temporary files into a chosen folder. Existing
files are preserved with a unique suffix, and a failure reports how many parts
were completed. A signed source produces parts with signatures that no longer
verify.

In **Organize**, **Add blank A4 page** appends a portrait page, while **Add
landscape A4 page** appends a horizontal one. Each page card can also insert
either orientation immediately before itself. Each insertion is one undoable
edit. The Rust core assigns its page ID and checks page-assembly permissions;
the preview is rebuilt and the new thumbnail loads asynchronously. Save the
document to keep the added page.

A "Form fields" panel fills in the AcroForm fields a document already has —
text, checkboxes, radio groups and dropdowns — gated on the fill permission
(ISO 32000-1 bit 6) rather than content editing. Each fill rebuilds the
preview, so pdfium paints the value from the regenerated appearance; the shell
draws no overlay of its own. The panel can also place single-line text fields,
checkboxes, two-option radio groups and non-editable dropdowns on a page.
New radio groups and dropdowns start with "Option 1" and "Option 2", as on
Linux. The core assigns each field's name and ID; placement is undoable and
requires both annotation and content-edit permissions. Click to use the default
size, or drag to define a field's rectangle in either direction.
Field names can be edited in the panel; renaming also requires both permissions,
while documents that permit only filling keep their value controls available.
Text fields and dropdowns can also have their font family (Helvetica, Times
Roman or Courier), size (1–72 pt), and text color changed independently in the
panel. Styling is undoable and requires the same structural permissions as
renaming. The color picker records one edit when it closes.
Existing fields can also be moved by editing their X and Y coordinates in PDF
points. Moving preserves the field's size and is undoable under the same
structural permissions as renaming.
Their width and height can be edited in PDF points without moving their origin.
Resizing requires finite, positive dimensions and the same structural permissions;
it is undoable and refreshes the page preview.

Annotation tools show a pressed state while armed. Only one can be active;
click it again, or choose **Pointer**, to disarm it. Placing an annotation
returns to pointer mode, including a refused placement. Entering text editing,
placing a form field, organizing pages or opening another document also clears
the active annotation tool. Choosing a tool does not change the PDF or its
undo history.

**Resize annotation** sets the selected annotation's width and height in PDF
points, keeping its bottom-left PDF-space origin fixed. Dimensions must be finite
and positive; stamps stretch to the new rectangle. Ink has no rectangle and cannot
be resized. The edit requires annotation-edit permission, refreshes the overlay
and records one undo step. Cancel or unchanged dimensions preserve history,
including redo. Save to keep the change.

**Move annotation** sets the selected annotation's bottom-left X/Y in unrotated
PDF points (X increases rightward, Y upward), keeping its size. Ink uses the
bottom-left bounds of its points and moves every point by the same offset.
Coordinates must be finite; zero and negative values are allowed. The edit
requires annotation-edit permission, refreshes the overlay and records one undo
step. Cancel or an unchanged position preserves redo. Save to keep the change.

**Note** asks for text after you click or drag to choose its rectangle. Notes
accept multiple lines; **Add** becomes available when the text is not blank.
Cancel closes the prompt without creating an annotation or an undo step. Adding
records one undoable edit through the core; save to keep the note.

**Stamp** asks for a PNG or JPEG after you click or drag on a page. The image
fills the chosen rectangle; a different aspect ratio stretches to fit. Cancel,
unreadable files and invalid images create no annotation or undo step. A successful
placement selects the stamp and displays its image, with move, resize and undo/redo
available. Save to keep it. Dropping an image or pasting a clipboard bitmap keeps
its existing proportional default placement.

Select a note created in the current session with **Previous annotation** or
**Next annotation**, then choose
**Read note** to view its multiline text in a read-only dialog. Reading also works
when annotation editing is forbidden, and does not change the PDF or its history.
The dialog preserves the core's text, including blank note snapshots. Saving
keeps session notes available to read. After closing and reopening a PDF, existing
annotations are preserved in the file but are not yet listed by the core's session
snapshot, so this control cannot select those notes.

If Windows cancels a page gesture or the page loses pointer capture, its pending
annotation drag or form-field placement is discarded without an undo step. An
armed tool stays available for another attempt. Text selection stops extending
and retains the last sampled range.

**Previous annotation** and **Next annotation** cycle through the document's
annotations in their snapshot order, wrapping at either end. With nothing
selected, Previous starts at the last annotation and Next at the first.
The viewer reveals the selected annotation, including on another page or outside
the zoomed viewport, using the page's current rotation and scale. These controls
also work when annotation editing is forbidden, and let you select an annotation
hidden under another without changing the PDF or its undo history.

Build the native library and regenerate its matching bindings before building the
WinUI app:

```powershell
./build.ps1
& (& "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -requires Microsoft.Component.MSBuild -find MSBuild\**\Bin\MSBuild.exe | Select-Object -First 1) Pdf.Windows/Pdf.Windows.csproj -restore -p:Configuration=Debug -p:Platform=x64
```

`dotnet build` cannot build this project: it fails to load the PRI packaging
tasks WinUI needs, so the shell must go through Visual Studio's MSBuild — which
is what `windows.yml` resolves with `vswhere` too. The build also fails with
MSB3021/MSB3027 while an instance of the app is running and holding `bin/`;
close it first.

`build.ps1` uses the pinned `uniffi-bindgen-cs v0.11.0+v0.31.0` installation to
generate bindings from the exact `pdf_ffi.dll` it copies beside the app. The
generated source and native DLL are local build artifacts and are not committed.

Facade behavior is checked without a WinUI runtime dependency:

```powershell
dotnet run --project Pdf.Windows.Facade.Tests/Pdf.Windows.Facade.Tests.csproj
```

## Note placement runtime smoke

`Tests/NotePlacementSmoke.targets` replaces the app entry point with an opt-in
WinUI harness. It exercises the real note dialog, cancel, blank validation,
placement geometry, undo/redo, control restoration and stale-session guards.
It also reads pending and saved session notes through the native snapshot adapter,
including empty and read-only snapshots, without adding history.
It writes `note-smoke.log` and `note-smoke.pdf` into the existing directory named
by `VITELA_SMOKE_OUTPUT`. A successful process exit alone is not a pass: the log
must start with `PASS`.

From the repository root, build the harness with Visual Studio MSBuild:

```powershell
$env:VITELA_SMOKE_OUTPUT = $env:TEMP
$env:PDFIUM_DYNAMIC_LIB_PATH = "$PWD\core\pdf-render\vendor\pdfium\bin\pdfium.dll"
& $msbuild apps/windows/Pdf.Windows/Pdf.Windows.csproj -restore -p:Configuration=Release -p:Platform=x64 -p:RuntimeIdentifier=win-x64 -p:SelfContained=true -p:WindowsAppSDKSelfContained=true "-p:CustomAfterMicrosoftCommonTargets=$PWD\apps\windows\Tests\NotePlacementSmoke.targets"
& ./apps/windows/Pdf.Windows/bin/x64/Release/net9.0-windows10.0.19041.0/win-x64/Pdf.Windows.exe
```

Here `$msbuild` is the Visual Studio MSBuild path resolved with `vswhere` above.
The saved note's `/Contents` should be exactly `  First line\rSecond line  `,
including the carriage return and two spaces at each end; inspect it with a PDF
parser such as pypdf. Rebuild without `CustomAfterMicrosoftCommonTargets` to
restore the normal app before packaging.

## Stamp placement runtime smoke

`Tests/StampPlacementSmoke.targets` runs the stamp flow in real WinUI with real
`StorageFile` reads and the native core. Only the picker response is substituted:
the operating system's file-selection UI still needs a manual check. The harness
covers PNG/JPEG insertion, chosen geometry, selection and preview, undo/redo,
save, cancellation, rejected files, stale-session guards and restored controls.

Use the same MSBuild and output directory setup as the note smoke, replacing
`NotePlacementSmoke.targets` with `StampPlacementSmoke.targets`, then run the app.
`stamp-smoke.log` must start with `PASS`; `stamp-smoke.pdf` contains the selected
two-pixel red/blue image as a stamp. Rebuild without the custom targets afterward.

## Annotation size runtime smoke

`Tests/AnnotationSizeSmoke.targets` exercises the real WinUI dimensions dialog
and native resize command. Use the note smoke setup with these targets, then run
the app; `annotation-size-smoke.log` must start with `PASS`. It checks validation,
cancel/no-op with redo preserved, exact geometry, undo/redo, save, stale snapshots
and guarded entry. `annotation-size-smoke.pdf` contains a shape at X 40, Y 80 with
width 123.5 and height 67.25 pt. Rebuild without custom targets afterward.

## Annotation position runtime smoke

`Tests/AnnotationPositionSmoke.targets` exercises the real WinUI position dialog
and native move command. Use the note smoke setup with these targets, then run
the app; it must exit with code 0 and `annotation-position-smoke.log` must start
with `PASS` without an `UNHANDLED` entry. It covers finite
input, negative/zero coordinates, cancel/no-op preserving redo, exact movement,
undo/redo, rotated pages, ink translation, stale snapshots and guarded entry.
`annotation-position-smoke.pdf` contains a shape at X 23.5, Y 47.25 with width
100 and height 40 pt, and ink points (-5, 0) and (15, 20), on a 90-degree page.
Rebuild without custom targets afterward.

## Page navigation runtime smoke

`Tests/PageNavigationSmoke.targets` exercises the real WinUI page-number dialog
and scrolling against a native document. Use the note smoke setup with these
targets and a fresh log destination, then run the app. It must exit with code 0;
`page-navigation-smoke.log` must start with `PASS` without an `UNHANDLED` entry.
It covers invalid numbers, cancellation, first/last pages at the current zoom,
hidden panels, preserved native undo/redo, stale page order/session and guarded
entry. It checks the Ctrl+G accelerator wiring; pressing the physical shortcut
still needs a manual check. It also invokes the Previous/Next page buttons,
checks first/last and single-page boundaries, scrolling, zoom, page-order changes
and restored controls after blocked entry. Rebuild without custom targets afterward.

## Packaging and signing

The distribution is a self-contained zip: the shell, the .NET and Windows App
SDK runtimes, `pdf_ffi.dll`, and **PDFium**. That last one is the reason the
packaging step exists at all. `pdf-render` resolves PDFium at runtime through
`PDFIUM_DYNAMIC_LIB_PATH`, then a path into the build machine's own
`core/pdf-render/vendor/pdfium` tree that is baked in at compile time, then the
bare library name. Only the first describes a shipped app — so
`Facade/BundledPdfium.cs` points the core at the copy staged beside the
executable, and a build that skips the staging renders on the machine that
produced it and nowhere else.

Build self-contained, then package:

```powershell
& (& "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -requires Microsoft.Component.MSBuild -find MSBuild\**\Bin\MSBuild.exe | Select-Object -First 1) Pdf.Windows/Pdf.Windows.csproj -restore -p:Configuration=Release -p:Platform=x64 -p:RuntimeIdentifier=win-x64 -p:SelfContained=true -p:WindowsAppSDKSelfContained=true
```

**Do not use `-t:Publish` here.** Publishing this project drops the compiled
XAML (`App.xbf`, `MainWindow.xbf`) and the app's resource index
(`Pdf.Windows.pri`); the published app then dies on its first frame inside
`Microsoft.UI.Xaml.dll` with a stowed `E_FAIL` (`0xC000027B`). The complete,
runnable app is the build output at
`Pdf.Windows\bin\x64\Release\<tfm>\win-x64`, which is what the packaging script
picks up — and it fails closed if those three files are missing.

Run the two scripts below from the repository root.

```powershell
./scripts/package-windows.ps1 -DevelopmentSigningCertificate
./scripts/verify-windows-package.ps1 -AllowUntrustedSignature
```

`package-windows.ps1` expects the pinned `pdfium-win-x64.tgz` at
`build/windows/tools/` (or `-PdfiumArchive`) and refuses anything that is not
the checksummed Windows/x64/non-V8/non-XFA build. `verify-windows-package.ps1`
works from the produced zip — contents, Authenticode signature, and a render of
page one performed by `Pdf.Windows.PackageSmoke` using nothing but the packaged
files, with `PDFIUM_DYNAMIC_LIB_PATH` cleared.

The two signing modes are not interchangeable. `-DevelopmentSigningCertificate`
mints a throwaway certificate no machine trusts; a release passes
`-SigningPfxBase64`/`-SigningPfxPassword`, which `windows.yml` supplies from the
`WINDOWS_SIGNING_PFX_BASE64` and `WINDOWS_SIGNING_PFX_PASSWORD` secrets. Until
those secrets exist, every CI package is development-signed and says so in the
job log.

## Microsoft Store

The Store build is the same app as an MSIX. The Store installs it, keeps every
copy up to date, and signs it, so it needs no certificate of ours. Everything
else stays unpackaged: `-p:VitelaStorePackage=true` is the only switch, and
nothing but `scripts/package-windows-store.ps1` sets it.

```powershell
./build.ps1 -Configuration Release        # from apps/windows
./scripts/package-windows-store.ps1       # from the repository root
```

The script checks the PDFium input with the same code as the zip
(`scripts/windows-pdfium.ps1`). It builds into `build/windows-store/`, with its
own `bin`/`obj`, and then opens the `.msix` it produced. It refuses the package
unless all of these hold:

- the identity and version are the ones requested;
- `pdfium.dll` is the pinned binary;
- `resources.pri` indexes `App.xbf` and `MainWindow.xbf`, because a packaged
  WinUI app carries its compiled XAML inside the PRI, not as loose files;
- the licenses are present;
- the Windows App SDK is declared as a framework dependency.

`.NET` is self-contained, but the Windows App SDK is not: the Store installs
that runtime alongside the app. The `windows.yml` package job runs the script
on every change.

The committed `Package.appxmanifest` carries a development identity. The real
`Name`, `Publisher` and `PublisherDisplayName` come from the Partner Center
reservation, and the script writes them, plus the version, into a build-time
copy.

To smoke-test the package locally, turn on Developer Mode and register the
unpacked layout. The package is unsigned, so it cannot be installed directly:

```powershell
Add-AppxPackage -Register build\windows-store\layout\AppxManifest.xml
```

### Releasing

Releases are cut with `scripts/release.sh` (see the root README), and the tag
is the only source of truth for the version. The tag starts `release.yml`,
which refuses it unless it points at a commit on `main` and then runs every
platform's tests. Only after all of them pass does it call
`windows-store.yml`, which:

1. receives the tag's MSIX translation (`scripts/release-version.sh <tag> msix`);
2. builds and checks the MSIX;
3. submits it with the Microsoft Store Developer CLI.

MSIX has no way to mark a prerelease, so the order alpha < beta < rc < final
is folded into the Build field (`patch×400 + rank×100 + N`). For example,
`v0.2.0-beta.1` becomes `0.2.101.0` and `v0.2.0` becomes `0.2.300.0`. A Store
update can never go backwards.

Certification takes from a few hours to a few business days. After that, the
Store updates installed copies on its own.

One-time setup, outside the repository:

1. Create a Partner Center developer account and reserve the app name.
2. Make the **first submission by hand**. That covers the listing, screenshots,
   age rating, privacy policy, and the `runFullTrust` justification. The
   workflow ships updates after it.
3. Register an Entra ID app and add it to Partner Center (Account settings →
   User management → Microsoft Entra applications) with the Manager role.
4. In GitHub, create the `microsoft-store` environment with:
   - variables: `STORE_PRODUCT_ID`, `STORE_IDENTITY_NAME`, `STORE_PUBLISHER`,
     `STORE_PUBLISHER_DISPLAY_NAME`, all from Partner Center → Product identity;
   - secrets: `PARTNER_CENTER_TENANT_ID`, `PARTNER_CENTER_SELLER_ID`,
     `PARTNER_CENTER_CLIENT_ID`, `PARTNER_CENTER_CLIENT_SECRET`.

The workflow checks that configuration first and names whatever is missing.

The Store tiles under `Pdf.Windows/Package/Images` are generated from the brand
mark with `python scripts/windows-store-images.py`.

## Diagnosing a reported failure

Failures shown to the user are deliberately vague — the shell never puts a
document's path, contents, or a decryption result on screen — and instead carry
a correlation id: *"The document could not be processed. Reference: fab46b34…"*.

Look that id up in

```
%LOCALAPPDATA%\Vitela\diagnostics.log
```

Each line carries a UTC timestamp, the failure category, the operation, the
correlation id, the session and page it happened on, and a sanitized detail —
usually the exception type. Document names, paths and contents are deliberately
absent, so the log can be attached to a bug report as-is. It is capped at 256 KB
with one previous generation kept beside it as `diagnostics.log.1`.
