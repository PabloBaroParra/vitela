#Requires -Version 5.1
<#
.SYNOPSIS
    Unpacks and verifies the pinned Windows PDFium input. Dot-source it.

.DESCRIPTION
    Shared by package-windows.ps1 (the signed zip) and
    package-windows-store.ps1 (the Microsoft Store MSIX). Both ship the same
    third-party binary, and the checks that prove it is the Windows/x64/
    non-V8/non-XFA build this project pinned must not drift between two
    copies of them.

    verify-windows-package.ps1 deliberately keeps its own checks: it is the
    independent witness of what a package contains, and sharing code with the
    script it audits would weaken that.

    The caller must define `Fail([string]$message)`, which is expected to throw;
    every check here reports through it, so failures keep the caller's prefix.
#>

$PDFIUM_VERSION = '148.0.7763.0'
$PDFIUM_ARCHIVE_SHA256 = '45c4cc5d052ef8ec6380b946b548a76100f4675e38362000a4c732e16d5e8eda'
$PDFIUM_DLL_SHA256 = 'a63949dc46a7314bba619ac6cc1b3849627e137f542ae31b2b36b302841f77ae'

function Require-File([string]$path, [string]$what) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { Fail "$what not found: $path" }
}

function Get-Sha256([string]$path) {
    (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
}

# Reads the COFF machine field straight out of the PE header. `file` and
# `readelf` are what the Linux script leans on; Windows runners ship neither,
# and the header is four well-defined bytes.
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

# Names of the DLLs a PE image imports (its import directory; delay-loaded
# imports are not listed). Parsed by hand for the same reason as above: no
# dumpbin on a bare runner.
function Get-PortableExecutableImports([string]$path) {
    $bytes = [System.IO.File]::ReadAllBytes($path)
    $pe = [System.BitConverter]::ToInt32($bytes, 0x3C)
    $sectionCount = [System.BitConverter]::ToUInt16($bytes, $pe + 6)
    $optional = $pe + 24
    $optionalSize = [System.BitConverter]::ToUInt16($bytes, $pe + 20)
    if ([System.BitConverter]::ToUInt16($bytes, $optional) -ne 0x20B) { Fail "$path is not a PE32+ image" }
    $sections = $optional + $optionalSize
    $toOffset = {
        param([uint32]$rva)
        for ($i = 0; $i -lt $sectionCount; $i++) {
            $s = $sections + 40 * $i
            $va = [System.BitConverter]::ToUInt32($bytes, $s + 12)
            $size = [Math]::Max([System.BitConverter]::ToUInt32($bytes, $s + 8), [System.BitConverter]::ToUInt32($bytes, $s + 16))
            if ($rva -ge $va -and $rva -lt $va + $size) { return [int]($rva - $va + [System.BitConverter]::ToUInt32($bytes, $s + 20)) }
        }
        Fail "$path has an import RVA outside every section"
    }
    $importRva = [System.BitConverter]::ToUInt32($bytes, $optional + 112 + 8)
    if ($importRva -eq 0) { return @() }
    $names = @()
    for ($d = & $toOffset $importRva; ; $d += 20) {
        $nameRva = [System.BitConverter]::ToUInt32($bytes, $d + 12)
        if ($nameRva -eq 0) { break }
        $start = & $toOffset $nameRva
        $end = [Array]::IndexOf($bytes, [byte]0, $start)
        $names += [System.Text.Encoding]::ASCII.GetString($bytes, $start, $end - $start)
    }
    $names
}

# A packaged native library must link the C runtime statically. Windows ships
# the Universal CRT (the api-ms-win-crt-* sets) but not VCRUNTIME140.dll. The
# MSIX declares no VCLibs dependency, and a developer machine or CI runner
# always has it from Visual Studio, so the gap stays hidden right up to a clean
# install. There, the app starts, and the first core call (opening a PDF) fails
# to load pdf_ffi.dll. Store certification rejected 0.1.101.0 for exactly that.
# The core is built with +crt-static (apps/windows/build.ps1), which removes every
# CRT import, so any CRT import at all means that build setting was lost.
function Assert-NoVisualCRuntimeImports([string]$path, [string]$what) {
    $runtime = @(Get-PortableExecutableImports $path | Where-Object { $_ -match '^(vcruntime|msvcp|ucrtbase|api-ms-win-crt-)' })
    if ($runtime) { Fail "$what imports the C runtime dynamically (VCRUNTIME140.dll is not part of Windows; build with +crt-static): $($runtime -join ', ')" }
}

# Expands -Archive into -Destination (which must exist and be empty) and fails
# closed unless every fact about it matches the pin. Returns the paths the
# packagers copy from.
function Expand-PinnedPdfium([string]$Archive, [string]$Destination) {
    Require-File $Archive 'PDFium archive'
    if ((Get-Sha256 $Archive) -ne $PDFIUM_ARCHIVE_SHA256) { Fail 'PDFium archive checksum mismatch' }

    # bsdtar ships in Windows 10 1803 and later, so no extra tool is required.
    # Addressed by full path rather than through PATH: a developer machine with Git
    # or MSYS installed resolves `tar` to GNU tar, which reads "D:\..." as a remote
    # host and fails with "Cannot connect to D:".
    $tar = Join-Path $env:SystemRoot 'System32\tar.exe'
    Require-File $tar 'Windows tar'
    & $tar -xzf $Archive -C $Destination
    if ($LASTEXITCODE -ne 0) { Fail 'PDFium archive is unreadable' }

    $pdfiumDll = Join-Path $Destination 'bin\pdfium.dll'
    $pdfiumLicense = Join-Path $Destination 'LICENSE'
    $pdfiumNotices = Join-Path $Destination 'licenses'
    Require-File $pdfiumDll 'PDFium library'
    Require-File $pdfiumLicense 'PDFium license'
    Require-File (Join-Path $Destination 'VERSION') 'PDFium version metadata'
    Require-File (Join-Path $Destination 'args.gn') 'PDFium build arguments'
    if (-not (Get-ChildItem -LiteralPath $pdfiumNotices -File -ErrorAction SilentlyContinue)) {
        Fail 'PDFium archive lacks third-party notices'
    }

    $version = @{}
    foreach ($line in (Get-Content -LiteralPath (Join-Path $Destination 'VERSION'))) {
        if ($line -match '^(MAJOR|MINOR|BUILD|PATCH)=([0-9]+)$') { $version[$Matches[1]] = $Matches[2] }
    }
    if ($version.Count -ne 4) { Fail 'PDFium VERSION metadata is malformed' }
    $archiveVersion = "$($version.MAJOR).$($version.MINOR).$($version.BUILD).$($version.PATCH)"
    if ($archiveVersion -ne $PDFIUM_VERSION) { Fail "PDFium version is $archiveVersion, not $PDFIUM_VERSION" }

    # Same four build-configuration facts the Linux packaging script insists on:
    # the right platform, and neither of the two attack-surface features this
    # project has never enabled.
    $buildArgs = Get-Content -LiteralPath (Join-Path $Destination 'args.gn')
    function Assert-BuildArgument([string]$pattern, [string]$message) {
        if (-not ($buildArgs | Where-Object { $_ -match $pattern })) { Fail $message }
    }
    Assert-BuildArgument '^\s*target_os\s*=\s*"win"\s*$' 'PDFium input is not Windows'
    Assert-BuildArgument '^\s*target_cpu\s*=\s*"x64"\s*$' 'PDFium input is not x64'
    Assert-BuildArgument '^\s*pdf_enable_v8\s*=\s*false\s*$' 'PDFium input enables V8'
    Assert-BuildArgument '^\s*pdf_enable_xfa\s*=\s*false\s*$' 'PDFium input enables XFA'
    if ($buildArgs | Where-Object { $_ -match '^\s*pdf_enable_v8\s*=\s*true' }) { Fail 'PDFium input enables V8' }

    Assert-PortableExecutableIsX64 $pdfiumDll 'PDFium library'
    if ((Get-Sha256 $pdfiumDll) -ne $PDFIUM_DLL_SHA256) { Fail 'PDFium library checksum mismatch' }

    [pscustomobject]@{
        Dll     = $pdfiumDll
        License = $pdfiumLicense
        Notices = $pdfiumNotices
    }
}
