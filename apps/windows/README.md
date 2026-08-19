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

The current vertical opens a local PDF — either through the file picker or the
**Open sample** button, which loads the shared sample document copied beside
the executable as `Assets\vitela-sample.pdf` (see
[`assets/README.md`](../../assets/README.md)) — lazily renders its pages, and
supports case-sensitive exact-text search. Selecting a result navigates to its page and highlights the
matching PDF-space character geometry. It intentionally does not include
editing, saving, password UI, or update logic. CI runs in
`.github/workflows/windows.yml`: the Rust workspace on a Windows runner, the
facade suite, and a full shell build (native dll + regenerated bindings +
MSBuild).

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
