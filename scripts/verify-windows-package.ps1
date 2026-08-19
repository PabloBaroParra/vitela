#Requires -Version 5.1
<#
.SYNOPSIS
    Verifies an assembled Windows package: contents, signature, offline render (T-064).

.DESCRIPTION
    The counterpart of scripts/verify-linux-package.sh. It works from the zip,
    not from the staging directory, so what it inspects is what a reader would
    download.

    Three questions, in order:

      1. Does the archive contain the files a working install needs — the
         shell, its FFI library, the bundled PDFium, its licences?
      2. Is every binary this project produces or bundles Authenticode-signed?
      3. Do those files actually render a page, with no vendored PDFium tree
         and no PDFIUM_DYNAMIC_LIB_PATH in the environment to rescue them?

    The third is the one that catches the failure this task exists to fix, and
    it needs a binary the package cannot provide: a WinUI app has no console
    and no headless render path. Pdf.Windows.PackageSmoke is that binary, and
    it compiles in the shell's own PDFium locator so the resolution being
    proven is the shipped one. The packaged files are copied out to it rather
    than it being copied into the package, which stays untouched.
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$PackagesDir,
    [string]$EvidenceDir,
    [string]$PackageVersion,
    [string]$SmokeProject,
    # Accept a signature whose chain the machine does not trust — the expected
    # outcome of a development-certificate signing run, and never of a release.
    [switch]$AllowUntrustedSignature,
    # Contents and signature only. Used where no .NET runtime is available to
    # host the smoke harness.
    [switch]$InspectOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# The packaged library carries a signature this script's own pipeline applied,
# so its bytes no longer hash to the archive's. Its identity is asserted from
# what signing does not touch: the version resource and the COFF header.
# package-windows.ps1 pins the checksum, where the file is still untouched.
$PDFIUM_VERSION = '148.0.7763.0'

function Fail([string]$message) { throw "verify-windows-package: $message" }

function Assert-PortableExecutableIsX64([string]$path, [string]$what) {
    $bytes = [System.IO.File]::ReadAllBytes($path)
    if ($bytes.Length -lt 0x40) { Fail "$what is too small to be a PE image" }
    if ($bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) { Fail "$what is not a PE image" }
    $peOffset = [System.BitConverter]::ToInt32($bytes, 0x3C)
    if ($peOffset -le 0 -or ($peOffset + 6) -ge $bytes.Length) { Fail "$what has a malformed PE header offset" }
    if ($bytes[$peOffset] -ne 0x50 -or $bytes[$peOffset + 1] -ne 0x45) { Fail "$what has no PE signature" }
    $machine = [System.BitConverter]::ToUInt16($bytes, $peOffset + 4)
    if ($machine -ne 0x8664) { Fail ("{0} is not an x64 image (COFF machine 0x{1:X4})" -f $what, $machine) }
}

function Require-PackagedFile([string]$path) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { Fail "required package entry missing: $path" }
}

function Get-Sha256([string]$path) {
    (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
}

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$buildRoot = Join-Path $repositoryRoot 'build\windows'
if (-not $PackagesDir) { $PackagesDir = Join-Path $buildRoot 'packages' }
if (-not $EvidenceDir) { $EvidenceDir = Join-Path $buildRoot 'evidence' }
if (-not $SmokeProject) {
    $SmokeProject = Join-Path $repositoryRoot 'apps\windows\Pdf.Windows.PackageSmoke\Pdf.Windows.PackageSmoke.csproj'
}
if (-not $PackageVersion) {
    $manifest = Get-Content -LiteralPath (Join-Path $repositoryRoot 'Cargo.toml')
    $versionLine = $manifest | Where-Object { $_ -match '^version = "(.+)"$' } | Select-Object -First 1
    # Matched again in this scope on purpose: $Matches from inside a
    # Where-Object block is not a contract worth naming a release artifact after.
    if (-not $versionLine -or $versionLine -notmatch '^version = "(.+)"$') {
        Fail 'workspace version not found in Cargo.toml'
    }
    $PackageVersion = $Matches[1]
}

$zipPath = Join-Path $PackagesDir "Vitela-$PackageVersion-x86_64-windows.zip"
if (-not (Test-Path -LiteralPath $zipPath -PathType Leaf)) { Fail "package archive not found: $zipPath" }

if (Test-Path -LiteralPath $EvidenceDir) { Remove-Item -LiteralPath $EvidenceDir -Recurse -Force }
$extractDir = Join-Path $EvidenceDir 'package'
$smokeDir = Join-Path $EvidenceDir 'smoke'
New-Item -ItemType Directory -Force -Path $extractDir, $smokeDir | Out-Null

Expand-Archive -LiteralPath $zipPath -DestinationPath $extractDir -Force
$installDir = Join-Path $extractDir 'Vitela'
if (-not (Test-Path -LiteralPath $installDir -PathType Container)) {
    Fail 'package archive does not contain a Vitela directory'
}

Get-ChildItem -LiteralPath $installDir -Recurse -File |
    ForEach-Object { $_.FullName.Substring($installDir.Length + 1) } |
    Set-Content -LiteralPath (Join-Path $EvidenceDir 'package-listing.txt')

foreach ($entry in @(
        'Pdf.Windows.exe',
        'Pdf.Windows.dll',
        # Compiled XAML and resource index: a package without them starts and
        # immediately dies inside Microsoft.UI.Xaml.dll.
        'App.xbf',
        'MainWindow.xbf',
        'Pdf.Windows.pri',
        'pdf_ffi.dll',
        'pdfium.dll',
        'Microsoft.WindowsAppRuntime.Bootstrap.dll',
        'coreclr.dll',
        'Assets\vitela-sample.pdf',
        'licenses\LICENSE-MIT',
        'licenses\LICENSE-APACHE',
        'licenses\pdfium\LICENSE')) {
    Require-PackagedFile (Join-Path $installDir $entry)
}
if (-not (Get-ChildItem -LiteralPath (Join-Path $installDir 'licenses\pdfium') -File | Where-Object { $_.Name -ne 'LICENSE' })) {
    Fail 'package lacks PDFium third-party notices'
}
if (Get-ChildItem -LiteralPath $installDir -Recurse -File -Filter '*.pdb') {
    Fail 'package ships debug symbols'
}
$packagedPdfium = Join-Path $installDir 'pdfium.dll'
Assert-PortableExecutableIsX64 $packagedPdfium 'packaged PDFium'
Assert-PortableExecutableIsX64 (Join-Path $installDir 'Pdf.Windows.exe') 'packaged shell executable'
Assert-PortableExecutableIsX64 (Join-Path $installDir 'pdf_ffi.dll') 'packaged FFI library'
$packagedPdfiumVersion = (Get-Item -LiteralPath $packagedPdfium).VersionInfo.FileVersion
if ($packagedPdfiumVersion -ne $PDFIUM_VERSION) {
    Fail "packaged PDFium reports version $packagedPdfiumVersion, not $PDFIUM_VERSION"
}

$signed = @('Pdf.Windows.exe', 'Pdf.Windows.dll', 'pdf_ffi.dll', 'pdfium.dll')
$signatureReport = foreach ($name in $signed) {
    $signature = Get-AuthenticodeSignature -LiteralPath (Join-Path $installDir $name)
    if (-not $signature.SignerCertificate) { Fail "$name is not Authenticode-signed" }
    if ($signature.Status -ne 'Valid') {
        if (-not $AllowUntrustedSignature) {
            Fail "$name has an unusable signature: $($signature.Status) $($signature.StatusMessage)"
        }
        # A development-certificate run reaches here by design; an unsigned or
        # tampered file does not, because it has no signer certificate at all.
        if ($signature.Status -ne 'UnknownError' -and $signature.Status -ne 'NotTrusted') {
            Fail "$name has a broken signature: $($signature.Status) $($signature.StatusMessage)"
        }
    }
    "$name status=$($signature.Status) signer=$($signature.SignerCertificate.Subject)"
}
$signatureReport | Set-Content -LiteralPath (Join-Path $EvidenceDir 'signatures.txt')

Get-Sha256 $zipPath | Set-Content -LiteralPath (Join-Path $EvidenceDir 'artifact-sha256.txt')

if ($InspectOnly) {
    'verified Windows x64 package contents and signatures' | Set-Content -LiteralPath (Join-Path $EvidenceDir 'result.txt')
    Write-Host 'verify-windows-package: contents and signatures verified (smoke skipped)'
    return
}

& dotnet publish $SmokeProject -c Release -o $smokeDir --nologo | Out-Null
if ($LASTEXITCODE -ne 0) { Fail 'the package smoke harness failed to build' }

# The harness runs on the packaged files, not on its own build's inputs.
foreach ($runtimeFile in @('pdfium.dll', 'pdf_ffi.dll')) {
    Copy-Item -LiteralPath (Join-Path $installDir $runtimeFile) -Destination $smokeDir -Force
}
New-Item -ItemType Directory -Force -Path (Join-Path $smokeDir 'Assets') | Out-Null
Copy-Item -LiteralPath (Join-Path $installDir 'Assets\vitela-sample.pdf') -Destination (Join-Path $smokeDir 'Assets') -Force

# Without this the check proves nothing: an override in the environment is the
# first thing the core honours, so a package missing PDFium entirely would
# still render.
$inheritedOverride = $env:PDFIUM_DYNAMIC_LIB_PATH
$env:PDFIUM_DYNAMIC_LIB_PATH = $null
$receiptPath = Join-Path $EvidenceDir 'package-smoke.txt'
try {
    & (Join-Path $smokeDir 'Pdf.Windows.PackageSmoke.exe') $receiptPath
    if ($LASTEXITCODE -ne 0) { Fail 'the packaged files did not render the sample document' }
}
finally {
    $env:PDFIUM_DYNAMIC_LIB_PATH = $inheritedOverride
}

$receipt = @{}
# UTF-8 explicitly: the harness writes the resolved library path, and Windows
# PowerShell otherwise reads the file in the console's ANSI codepage, which
# mangles any non-ASCII character in the install path.
foreach ($line in (Get-Content -LiteralPath $receiptPath -Encoding UTF8)) {
    if ($line -match '^([a-z_0-9]+)=(.*)$') { $receipt[$Matches[1]] = $Matches[2] }
}
foreach ($field in @('pdfium', 'width', 'height', 'pixels', 'ink', 'pixels_sha256')) {
    if (-not $receipt.ContainsKey($field)) { Fail "smoke receipt lacks $field" }
}
$expectedLibrary = (Join-Path $smokeDir 'pdfium.dll')
if (-not [string]::Equals($receipt['pdfium'], $expectedLibrary, [StringComparison]::OrdinalIgnoreCase)) {
    Fail "the smoke loaded $($receipt['pdfium']), not the packaged PDFium at $expectedLibrary"
}
if ([int]$receipt['width'] -le 0 -or [int]$receipt['height'] -le 0) { Fail 'smoke receipt has an empty page raster' }
if ([long]$receipt['ink'] -le 0) { Fail 'the packaged PDFium rendered a blank page' }

'verified Windows x64 package' | Set-Content -LiteralPath (Join-Path $EvidenceDir 'result.txt')
Write-Host "verify-windows-package: verified $zipPath"
Write-Host "verify-windows-package: evidence in $EvidenceDir"
