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

Push-Location $repositoryRoot
try {
    cargo build -p pdf-ffi $(if ($Configuration -eq "Release") { "--release" })
    uniffi-bindgen-cs --library $libraryPath --out-dir $generatedOutput
    Copy-Item -Force $libraryPath $nativeOutput
}
finally {
    Pop-Location
}
