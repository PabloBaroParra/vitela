#Requires -Version 5.1
<#
.SYNOPSIS
    Builds the Microsoft Store MSIX of the Vitela Windows shell.

.DESCRIPTION
    The Store counterpart of package-windows.ps1. Run apps/windows/build.ps1
    -Configuration Release first (native library + bindings); this script then
    builds the shell as an MSIX with the pinned PDFium inside it and checks the
    result before anything is uploaded.

    Why the Store build is packaged at all: the Store keeps an MSIX app up to
    date on its own and signs it, which is the point of shipping there. The
    unpackaged zip stays the direct-download distribution.

    The package is unsigned; Partner Center re-signs it. To install it locally
    for a smoke test, register the unpacked layout instead (Developer Mode):
        Add-AppxPackage -Register <BuildRoot>\layout\AppxManifest.xml

.PARAMETER Version
    Full MSIX version, Major.Minor.Build.0 (the Store reserves the revision).
    A release passes the tag's translation:
        bash scripts/release-version.sh v0.2.0-beta.1 msix    ->  0.2.101.0
    Without it the package is a development build, versioned 0.0.1.0: the
    release tag, not Cargo.toml, is the source of truth for real versions.

.PARAMETER IdentityName
    Package/Identity/Name from Partner Center (Product identity page).
    Defaults to the development placeholder in Package.appxmanifest.

.PARAMETER Publisher
    Package/Identity/Publisher from Partner Center, e.g. "CN=0A1B2C3D-...".

.PARAMETER PublisherDisplayName
    Package/Properties/PublisherDisplayName from Partner Center.
#>
[CmdletBinding()]
param(
    [string]$Version,
    [string]$IdentityName,
    [string]$Publisher,
    [string]$PublisherDisplayName,
    # Pinned pdfium-win-x64.tgz from bblanchon/pdfium-binaries.
    [string]$PdfiumArchive,
    [string]$BuildRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'windows-pdfium.ps1')

function Fail([string]$message) { throw "package-windows-store: $message" }

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$projectDir = Join-Path $repositoryRoot 'apps\windows\Pdf.Windows'
$project = Join-Path $projectDir 'Pdf.Windows.csproj'
if (-not $BuildRoot) { $BuildRoot = Join-Path $repositoryRoot 'build\windows-store' }
if (-not $PdfiumArchive) { $PdfiumArchive = $env:PDFIUM_ARCHIVE }
if (-not $PdfiumArchive) { $PdfiumArchive = Join-Path $repositoryRoot 'build\windows\tools\pdfium-win-x64.tgz' }

if (-not $Version) { $Version = '0.0.1.0' }
if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)\.0$') { Fail "version must be Major.Minor.Build.0, got '$Version'" }
foreach ($part in $Matches[1..3]) {
    if ([int]$part -gt 65535) { Fail "version field $part exceeds the MSIX limit of 65535" }
}
$packageVersion = $Version

Require-File (Join-Path $projectDir 'Native\pdf_ffi.dll') 'native library (run apps/windows/build.ps1 first)'

$workDir = Join-Path $BuildRoot 'work'
$packagesDir = Join-Path $BuildRoot 'packages'
$layoutDir = Join-Path $BuildRoot 'layout'
foreach ($stale in @($workDir, $packagesDir, $layoutDir, (Join-Path $BuildRoot 'bin'))) {
    if (Test-Path -LiteralPath $stale) { Remove-Item -LiteralPath $stale -Recurse -Force }
}
$pdfiumDir = Join-Path $workDir 'pdfium'
$licenseDir = Join-Path $workDir 'licenses'
New-Item -ItemType Directory -Force -Path $pdfiumDir, (Join-Path $licenseDir 'pdfium'), $packagesDir | Out-Null

$pdfium = Expand-PinnedPdfium $PdfiumArchive $pdfiumDir

# Same license set the zip carries.
Copy-Item -LiteralPath (Join-Path $repositoryRoot 'LICENSE-MIT') -Destination $licenseDir
Copy-Item -LiteralPath (Join-Path $repositoryRoot 'LICENSE-APACHE') -Destination $licenseDir
Copy-Item -LiteralPath $pdfium.License -Destination (Join-Path $licenseDir 'pdfium\LICENSE')
Copy-Item -Path (Join-Path $pdfium.Notices '*') -Destination (Join-Path $licenseDir 'pdfium') -Recurse

# A build-time copy of the manifest carries the identity and version, so the
# committed file never holds release values and a local run never dirties it.
$manifestXml = New-Object System.Xml.XmlDocument
$manifestXml.PreserveWhitespace = $true
$manifestXml.Load((Join-Path $projectDir 'Package.appxmanifest'))
$ns = New-Object System.Xml.XmlNamespaceManager $manifestXml.NameTable
$ns.AddNamespace('m', 'http://schemas.microsoft.com/appx/manifest/foundation/windows10')
$identity = $manifestXml.SelectSingleNode('/m:Package/m:Identity', $ns)
$identity.SetAttribute('Version', $packageVersion)
if ($IdentityName) { $identity.SetAttribute('Name', $IdentityName) }
if ($Publisher) {
    if ($Publisher -notmatch '^CN=') { Fail "publisher must be the full subject from Partner Center, starting with CN=" }
    $identity.SetAttribute('Publisher', $Publisher)
}
if ($PublisherDisplayName) {
    $manifestXml.SelectSingleNode('/m:Package/m:Properties/m:PublisherDisplayName', $ns).InnerText = $PublisherDisplayName
}
$manifestCopy = Join-Path $workDir 'Package.appxmanifest'
$manifestXml.Save($manifestCopy)

# Only Visual Studio's MSBuild carries the PRI/MSIX tasks WinUI needs.
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
Require-File $vswhere 'vswhere'
$msbuild = & $vswhere -latest -requires Microsoft.Component.MSBuild -find 'MSBuild\**\Bin\MSBuild.exe' | Select-Object -First 1
if (-not $msbuild) { Fail 'MSBuild not found via vswhere' }

# AppxPackageName names the .msix file instead of the project name. It replaces
# the whole base name, so the version and architecture are spelled out here to
# keep the usual Vitela.Windows_<version>_x64.msix shape. Only the file name
# changes: the Store reads the package identity from the manifest.
# Own bin/obj under BuildRoot: the zip packager copies the shared
# bin\x64\Release\<tfm>\win-x64 directory wholesale, and packaged and
# unpackaged builds must not leave artifacts in each other's output.
& $msbuild $project -restore -nologo -v:m `
    "-p:BaseOutputPath=$BuildRoot\bin\" "-p:BaseIntermediateOutputPath=$BuildRoot\obj\" `
    -p:Configuration=Release -p:Platform=x64 -p:RuntimeIdentifier=win-x64 -p:SelfContained=true `
    -p:VitelaStorePackage=true `
    "-p:VitelaAppxManifest=$manifestCopy" `
    "-p:VitelaPdfiumDll=$($pdfium.Dll)" `
    "-p:VitelaLicensesDir=$licenseDir" `
    "-p:AppxPackageDir=$packagesDir\" `
    "-p:AppxPackageName=Vitela.Windows_$($packageVersion)_x64"
if ($LASTEXITCODE -ne 0) { Fail "MSBuild failed with exit code $LASTEXITCODE" }

# Exact extension match: the build also leaves a .msixsym beside the package,
# and a Dependencies\ folder holding the Windows App SDK runtime packages,
# which the Store installs itself.
$msix = @(Get-ChildItem -LiteralPath $packagesDir -Recurse -File |
    Where-Object { $_.Extension -eq '.msix' -and $_.FullName -notlike '*\Dependencies\*' })
if ($msix.Count -ne 1) { Fail "expected one .msix under $packagesDir, found $($msix.Count)" }

# Check the package itself, not the build output: what the Store installs is
# what is inside this file.
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::ExtractToDirectory($msix[0].FullName, $layoutDir)

$requiredEntries = @(
    'AppxManifest.xml',
    'Pdf.Windows.exe',
    'Pdf.Windows.dll',
    'resources.pri',
    'pdf_ffi.dll',
    'pdfium.dll',
    # .NET is self-contained; the Windows App SDK is a framework dependency.
    'coreclr.dll',
    'Assets\vitela-sample.pdf',
    'Package\Images\StoreLogo.scale-100.png',
    'licenses\LICENSE-MIT',
    'licenses\LICENSE-APACHE',
    'licenses\pdfium\LICENSE'
)
foreach ($entry in $requiredEntries) {
    Require-File (Join-Path $layoutDir $entry) 'packaged file'
}

# A packaged WinUI app carries its compiled XAML inside resources.pri rather
# than as loose .xbf files. Missing XAML is the failure that looks complete
# until the app dies on its first frame, so check for it by name (the PRI
# stores these names as single-byte text).
$pri = [System.IO.File]::ReadAllBytes((Join-Path $layoutDir 'resources.pri'))
$priText = [System.Text.Encoding]::ASCII.GetString($pri)
foreach ($xbf in @('App.xbf', 'MainWindow.xbf')) {
    if (-not $priText.Contains($xbf)) { Fail "resources.pri does not index $xbf" }
}

Assert-PortableExecutableIsX64 (Join-Path $layoutDir 'Pdf.Windows.exe') 'packaged shell executable'
Assert-PortableExecutableIsX64 (Join-Path $layoutDir 'pdf_ffi.dll') 'packaged FFI library'
Assert-NoVisualCRuntimeImports (Join-Path $layoutDir 'pdf_ffi.dll') 'packaged FFI library'
Assert-NoVisualCRuntimeImports (Join-Path $layoutDir 'pdfium.dll') 'packaged PDFium'
if ((Get-Sha256 (Join-Path $layoutDir 'pdfium.dll')) -ne $PDFIUM_DLL_SHA256) { Fail 'packaged PDFium is not the pinned library' }

$packaged = [xml](Get-Content -LiteralPath (Join-Path $layoutDir 'AppxManifest.xml') -Raw)
$packagedIdentity = $packaged.Package.Identity
if ($packagedIdentity.Version -ne $packageVersion) { Fail "packaged version is $($packagedIdentity.Version), not $packageVersion" }
if ($packagedIdentity.ProcessorArchitecture -ne 'x64') { Fail "packaged architecture is $($packagedIdentity.ProcessorArchitecture), not x64" }
if ($IdentityName -and $packagedIdentity.Name -ne $IdentityName) { Fail "packaged identity is $($packagedIdentity.Name), not $IdentityName" }
if ($Publisher -and $packagedIdentity.Publisher -ne $Publisher) { Fail "packaged publisher is $($packagedIdentity.Publisher), not $Publisher" }
if (-not ($packaged.Package.Dependencies.PackageDependency | Where-Object { $_.Name -like 'Microsoft.WindowsAppRuntime.*' })) {
    Fail 'package does not declare its Windows App SDK framework dependency'
}

Write-Host "package-windows-store: $($packagedIdentity.Name) $packageVersion"
Write-Host "package-windows-store: wrote $($msix[0].FullName)"
if ($env:GITHUB_OUTPUT) {
    "msix=$($msix[0].FullName)" | Out-File -Append -Encoding utf8 $env:GITHUB_OUTPUT
    # msstore publish takes the directory, not the file; see windows-store.yml.
    "packages=$packagesDir" | Out-File -Append -Encoding utf8 $env:GITHUB_OUTPUT
    "version=$packageVersion" | Out-File -Append -Encoding utf8 $env:GITHUB_OUTPUT
}
