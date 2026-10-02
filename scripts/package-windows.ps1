#Requires -Version 5.1
<#
.SYNOPSIS
    Assembles the Windows x64 distribution of the Vitela shell (T-064).

.DESCRIPTION
    The counterpart of scripts/package-linux.sh. It does not build: hand it the
    self-contained shell build and the pinned PDFium archive, and it produces a
    signed zip under build/windows/packages.

    The PDFium input is checked before anything is copied, and every check
    fails closed. The archive is a third-party download, and "it extracted" is
    not evidence that it is the Windows/x64/non-V8/non-XFA build this project
    pinned.

    Bundling PDFium at all is the point of the task: nothing in the published
    shell carries it, and the core's fallback resolution ends at a compile-time
    path into the build machine's own vendor tree — which is why an unbundled
    build renders on the machine that produced it and nowhere else.
#>
[CmdletBinding()]
param(
    # Pinned pdfium-win-x64.tgz from bblanchon/pdfium-binaries.
    [string]$PdfiumArchive,
    # Self-contained shell *build* output — bin\x64\Release\<tfm>\win-x64.
    # Not a publish directory, and that is not a preference: `msbuild -t:Publish`
    # on this project drops the compiled XAML (App.xbf, MainWindow.xbf) and the
    # app's resource index (Pdf.Windows.pri), and the result crashes inside
    # Microsoft.UI.Xaml.dll the moment it starts (0xC000027B, a stowed
    # E_FAIL). The RID build output is the complete, runnable app.
    [string]$ShellOutputDir,
    [string]$BuildRoot,
    [string]$PackageVersion,
    # Authenticode inputs, forwarded verbatim to sign-windows-binaries.ps1.
    [string]$SigningPfxBase64,
    [string]$SigningPfxPassword,
    [string]$TimestampServer = 'http://timestamp.digicert.com',
    # Sign with a throwaway self-signed certificate instead. Development only:
    # the result is not distributable, exactly like the ad-hoc signed macOS
    # artifact macos.yml produces while T-059's real identity is missing.
    [switch]$DevelopmentSigningCertificate,
    [switch]$SkipSigning
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Pinned PDFium constants and its verification, shared with the MSIX packager.
. (Join-Path $PSScriptRoot 'windows-pdfium.ps1')

function Fail([string]$message) { throw "package-windows: $message" }

$repositoryRoot = Split-Path -Parent $PSScriptRoot
if (-not $BuildRoot) { $BuildRoot = Join-Path $repositoryRoot 'build\windows' }
if (-not $ShellOutputDir) {
    # Globbed rather than spelled out, so a target-framework bump does not
    # silently leave this script packaging a stale directory.
    $candidates = @(Get-ChildItem -Path (Join-Path $repositoryRoot 'apps\windows\Pdf.Windows\bin\x64\Release\*\win-x64') -Directory -ErrorAction SilentlyContinue)
    if ($candidates.Count -ne 1) {
        Fail "expected exactly one self-contained shell build under apps\windows\Pdf.Windows\bin\x64\Release\*\win-x64, found $($candidates.Count)"
    }
    $ShellOutputDir = $candidates[0].FullName
}
if (-not $PdfiumArchive) { $PdfiumArchive = $env:PDFIUM_ARCHIVE }
if (-not $PdfiumArchive) { $PdfiumArchive = Join-Path $BuildRoot 'tools\pdfium-win-x64.tgz' }
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

Require-File $PdfiumArchive 'PDFium archive'
if (-not (Test-Path -LiteralPath $ShellOutputDir -PathType Container)) {
    Fail "self-contained shell build not found: $ShellOutputDir"
}

$workDir = Join-Path $BuildRoot 'work'
$stageRoot = Join-Path $BuildRoot 'stage'
$packagesDir = Join-Path $BuildRoot 'packages'
$pdfiumDir = Join-Path $workDir 'pdfium'
$stageDir = Join-Path $stageRoot 'Vitela'

foreach ($stale in @($workDir, $stageRoot, $packagesDir)) {
    if (Test-Path -LiteralPath $stale) { Remove-Item -LiteralPath $stale -Recurse -Force }
}
New-Item -ItemType Directory -Force -Path $pdfiumDir, $stageDir, $packagesDir | Out-Null

$pdfium = Expand-PinnedPdfium $PdfiumArchive $pdfiumDir
$pdfiumDll = $pdfium.Dll
$pdfiumLicense = $pdfium.License
$pdfiumNotices = $pdfium.Notices

# The published shell, minus its symbols: a .pdb maps every private symbol in
# the app and belongs with the build, not with the reader.
Copy-Item -Path (Join-Path $ShellOutputDir '*') -Destination $stageDir -Recurse -Force
Get-ChildItem -LiteralPath $stageDir -Recurse -Filter '*.pdb' | Remove-Item -Force

Copy-Item -LiteralPath $pdfiumDll -Destination (Join-Path $stageDir 'pdfium.dll') -Force

$licenseDir = Join-Path $stageDir 'licenses'
$pdfiumLicenseDir = Join-Path $licenseDir 'pdfium'
New-Item -ItemType Directory -Force -Path $pdfiumLicenseDir | Out-Null
Copy-Item -LiteralPath (Join-Path $repositoryRoot 'LICENSE-MIT') -Destination $licenseDir -Force
Copy-Item -LiteralPath (Join-Path $repositoryRoot 'LICENSE-APACHE') -Destination $licenseDir -Force
Copy-Item -LiteralPath $pdfiumLicense -Destination (Join-Path $pdfiumLicenseDir 'LICENSE') -Force
Copy-Item -Path (Join-Path $pdfiumNotices '*') -Destination $pdfiumLicenseDir -Recurse -Force

# What the package is required to contain. Stated here rather than left to the
# verification script alone, so a broken publish fails at the step that made it.
$requiredEntries = @(
    'Pdf.Windows.exe',
    'Pdf.Windows.dll',
    # The compiled XAML and the app's resource index. Listed because their
    # absence is silent: everything else about such a build looks right, and
    # the app dies inside Microsoft.UI.Xaml.dll on the first frame. That is
    # exactly what `msbuild -t:Publish` produces for this project.
    'App.xbf',
    'MainWindow.xbf',
    'Pdf.Windows.pri',
    'pdf_ffi.dll',
    'pdfium.dll',
    # Proof the publish was self-contained. A framework-dependent zip looks
    # complete and then asks the reader to go install two runtimes.
    'Microsoft.WindowsAppRuntime.Bootstrap.dll',
    'coreclr.dll',
    'Assets\vitela-sample.pdf',
    'licenses\LICENSE-MIT',
    'licenses\LICENSE-APACHE',
    'licenses\pdfium\LICENSE'
)
foreach ($entry in $requiredEntries) {
    Require-File (Join-Path $stageDir $entry) "packaged file"
}
Assert-PortableExecutableIsX64 (Join-Path $stageDir 'Pdf.Windows.exe') 'packaged shell executable'
Assert-PortableExecutableIsX64 (Join-Path $stageDir 'pdf_ffi.dll') 'packaged FFI library'

if (-not $SkipSigning) {
    & (Join-Path $PSScriptRoot 'sign-windows-binaries.ps1') `
        -StageDir $stageDir `
        -PfxBase64 $SigningPfxBase64 `
        -PfxPassword $SigningPfxPassword `
        -TimestampServer $TimestampServer `
        -DevelopmentCertificate:$DevelopmentSigningCertificate
}

$zipPath = Join-Path $packagesDir "Vitela-$PackageVersion-x86_64-windows.zip"
Compress-Archive -Path $stageDir -DestinationPath $zipPath -CompressionLevel Optimal
Require-File $zipPath 'package archive'

Write-Host "package-windows: staged $stageDir"
Write-Host "package-windows: wrote $zipPath"
