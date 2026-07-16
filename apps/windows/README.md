# Windows shell

`Pdf.Windows` is the WinUI 3 shell. Its views use only the handwritten C# facade;
generated UniFFI types are isolated in `Facade/GeneratedPdfCore.cs`.

## First vertical

The current vertical opens a local PDF, lazily renders its pages, and supports
case-sensitive exact-text search. Selecting a result navigates to its page and highlights the
matching PDF-space character geometry. It intentionally does not include
editing, saving, password UI, update logic, or CI workflows.

Build the native library and regenerate its matching bindings before building the
WinUI app:

```powershell
./build.ps1
dotnet build Pdf.Windows/Pdf.Windows.csproj
```

`build.ps1` uses the pinned `uniffi-bindgen-cs v0.11.0+v0.31.0` installation to
generate bindings from the exact `pdf_ffi.dll` it copies beside the app. The
generated source and native DLL are local build artifacts and are not committed.

Facade behavior is checked without a WinUI runtime dependency:

```powershell
dotnet run --project Pdf.Windows.Facade.Tests/Pdf.Windows.Facade.Tests.csproj
```
