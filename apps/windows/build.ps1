param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Debug"
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$profile = $Configuration.ToLowerInvariant()
$nativeOutput = Join-Path $PSScriptRoot "Pdf.Windows\Native"
$generatedOutput = Join-Path $PSScriptRoot "Pdf.Windows\Generated"
$libraryPath = Join-Path $repositoryRoot "target\$profile\pdf_ffi.dll"

New-Item -ItemType Directory -Force -Path $nativeOutput, $generatedOutput | Out-Null

# Link the C runtime statically. A default MSVC build imports VCRUNTIME140.dll,
# which Windows does not ship and the MSIX does not declare. The app then fails
# on its first PDF on any machine without Visual Studio or the VC++
# redistributable, and Store certification rejected 0.1.101.0 for it. Appended,
# never replacing: CI sets RUSTFLAGS (-D warnings), and an environment
# RUSTFLAGS also overrides any .cargo/config.toml rustflags, which is why the
# flag lives here. The packagers refuse a build without it
# (Assert-NoVisualCRuntimeImports in scripts/windows-pdfium.ps1).
$env:RUSTFLAGS = ("$env:RUSTFLAGS -C target-feature=+crt-static").Trim()

Push-Location $repositoryRoot
try {
    cargo build -p pdf-ffi $(if ($Configuration -eq "Release") { "--release" })
    uniffi-bindgen-cs --library $libraryPath --out-dir $generatedOutput
    Copy-Item -Force $libraryPath $nativeOutput
}
finally {
    Pop-Location
}
