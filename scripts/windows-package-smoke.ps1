#Requires -Version 5.1
<#
.SYNOPSIS
    Renders the sample document with a package's own native files, optionally
    on a runner that has been stripped of the Visual C++ runtime. Dot-source it.

.DESCRIPTION
    Shared by verify-windows-package.ps1 (the zip) and
    verify-windows-store-package.ps1 (the Microsoft Store MSIX). Both
    verifiers have to answer the same question, "do the shipped files render
    a page on a machine that never built them?", so they share one answer.
    The packagers do not use this file. A verifier is the independent witness
    of what a packager produced.

    Why a clean machine has to be simulated: a GitHub Windows runner has
    Visual Studio, so System32 holds vcruntime140.dll and PATH lists tool
    directories that ship more copies. A smoke run there passed in green while
    pdf_ffi.dll imported VCRUNTIME140.dll, and Store certification then failed
    "Open PDF" on a clean laptop. -CleanMachine hides the runtime from
    System32, runs the smoke with a PATH reduced to Windows' own directories,
    and first proves the runtime really is unreachable.

    The caller must define `Fail([string]$message)`, which is expected to throw.
#>

# Every DLL of the Visual C++ 2015-2022 redistributable that a native library
# could import. The Universal CRT (ucrtbase, api-ms-win-crt-*) is part of
# Windows 10 and later, so it stays.
$VC_RUNTIME_PATTERNS = @('vcruntime140*.dll', 'msvcp140*.dll', 'concrt140.dll', 'vccorlib140.dll', 'vcomp140.dll')

# The patterns also match the .NET Framework's private copies
# (vcruntime140_clr0400.dll, msvcp140_clr0400.dll and so on). Windows itself
# installs those, so a clean machine has them, and Windows PowerShell cannot
# start without them. Hiding them failed CI with ".NET Framework v4.0.30319 is
# not installed".
function Test-IsVisualCRedistributableFile([System.IO.FileInfo]$file) {
    $file.Name -notlike '*_clr0400.dll'
}

# Changing System32 or the machine's certificate trust is only acceptable on
# a VM that is destroyed when the job ends.
function Assert-DisposableRunner([string]$what) {
    if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
        Fail "$what changes this machine and runs only on a GitHub-hosted runner"
    }
}

# Windows' own directories only. The smoke runs with this PATH, so a copy of
# the runtime in some tool directory cannot rescue a broken package.
function Get-WindowsOnlyPath {
    $windows = $env:SystemRoot
    @("$windows\System32", $windows, "$windows\System32\Wbem", "$windows\System32\WindowsPowerShell\v1.0") -join ';'
}

function Hide-VisualCRuntime {
    Assert-DisposableRunner 'hiding the Visual C++ runtime'
    $system32 = Join-Path $env:SystemRoot 'System32'
    $hidden = @()
    try {
        foreach ($pattern in $VC_RUNTIME_PATTERNS) {
            foreach ($file in @(Get-ChildItem -LiteralPath $system32 -Filter $pattern -File | Where-Object { Test-IsVisualCRedistributableFile $_ })) {
                # TrustedInstaller owns these. Renaming a DLL that a running
                # process has mapped is allowed; deleting it would not be.
                & takeown.exe /f $file.FullName | Out-Null
                & icacls.exe $file.FullName /grant '*S-1-5-32-544:F' | Out-Null
                $target = "$($file.FullName).hidden-by-ci"
                Rename-Item -LiteralPath $file.FullName -NewName (Split-Path -Leaf $target)
                $hidden += $target
            }
        }
    }
    catch {
        # The caller never receives a partial list, so undo it here.
        Restore-VisualCRuntime $hidden
        throw
    }
    if (-not $hidden) { Fail 'no Visual C++ runtime found in System32 to hide; the clean-machine run would prove nothing' }
    $hidden
}

function Restore-VisualCRuntime([string[]]$hidden) {
    foreach ($path in $hidden) {
        if (Test-Path -LiteralPath $path) {
            Rename-Item -LiteralPath $path -NewName ((Split-Path -Leaf $path) -replace '\.hidden-by-ci$', '')
        }
    }
}

# The negative control. A fresh process with the reduced PATH tries to load
# vcruntime140.dll, and the load must fail. If it succeeds, the simulation is
# broken, and a green smoke would mean nothing.
function Assert-VisualCRuntimeUnreachable([string]$path) {
    # Exit 3: the runtime loaded. Exit 4: the probe itself broke. Only a clean
    # 0 counts, so a probe that cannot run is never mistaken for a pass.
    $probe = @'
$ErrorActionPreference = 'Stop'
try {
    Add-Type -Namespace Probe -Name Native -MemberDefinition '[System.Runtime.InteropServices.DllImport("kernel32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)] public static extern System.IntPtr LoadLibraryW(string name);'
} catch { exit 4 }
if ([Probe.Native]::LoadLibraryW('vcruntime140.dll') -ne [System.IntPtr]::Zero) { exit 3 }
exit 0
'@
    # Encoded, not -Command: Windows PowerShell drops the embedded double quotes
    # when it passes a string argument to a native program, which silently broke
    # the probe into a pass.
    $encoded = [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes($probe))
    $saved = $env:PATH
    $env:PATH = $path
    try {
        & (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -NoProfile -NonInteractive -EncodedCommand $encoded
        $code = $LASTEXITCODE
    }
    finally {
        $env:PATH = $saved
    }
    if ($code -eq 3) { Fail 'vcruntime140.dll still loads after hiding it; the clean-machine simulation is not clean' }
    if ($code -ne 0) { Fail "the Visual C++ runtime probe failed to run (exit $code)" }
}

# Renders page one of the sample with $InstallDir's pdf_ffi.dll and
# pdfium.dll, through Pdf.Windows.PackageSmoke, and asserts on its receipt.
function Invoke-PackageSmoke([string]$InstallDir, [string]$EvidenceDir, [string]$SmokeProject, [switch]$CleanMachine) {
    $smokeDir = Join-Path $EvidenceDir 'smoke'
    New-Item -ItemType Directory -Force -Path $smokeDir | Out-Null
    & dotnet publish $SmokeProject -c Release -o $smokeDir --nologo | Out-Null
    if ($LASTEXITCODE -ne 0) { Fail 'the package smoke harness failed to build' }

    # The harness runs on the packaged files, not on its own build's inputs.
    foreach ($runtimeFile in @('pdfium.dll', 'pdf_ffi.dll')) {
        Copy-Item -LiteralPath (Join-Path $InstallDir $runtimeFile) -Destination $smokeDir -Force
    }
    New-Item -ItemType Directory -Force -Path (Join-Path $smokeDir 'Assets') | Out-Null
    Copy-Item -LiteralPath (Join-Path $InstallDir 'Assets\vitela-sample.pdf') -Destination (Join-Path $smokeDir 'Assets') -Force
    # The application directory is searched first, so a runtime that landed
    # beside the harness would rescue the package as surely as System32 would.
    foreach ($pattern in $VC_RUNTIME_PATTERNS) {
        if (Get-ChildItem -LiteralPath $smokeDir -Filter $pattern -File) { Fail "the smoke directory carries $pattern" }
    }

    # Without this the check proves nothing: an override in the environment is
    # the first thing the core honours, so a package missing PDFium entirely
    # would still render.
    $inheritedOverride = $env:PDFIUM_DYNAMIC_LIB_PATH
    $inheritedPath = $env:PATH
    $env:PDFIUM_DYNAMIC_LIB_PATH = $null
    $receiptPath = Join-Path $EvidenceDir 'package-smoke.txt'
    $hidden = @()
    try {
        if ($CleanMachine) {
            $hidden = Hide-VisualCRuntime
            $cleanPath = Get-WindowsOnlyPath
            Assert-VisualCRuntimeUnreachable $cleanPath
            # The harness is framework-dependent. Its host finds .NET through
            # the registered install location, not through PATH.
            $env:PATH = $cleanPath
        }
        & (Join-Path $smokeDir 'Pdf.Windows.PackageSmoke.exe') $receiptPath
        if ($LASTEXITCODE -ne 0) { Fail 'the packaged files did not render the sample document' }
    }
    finally {
        $env:PATH = $inheritedPath
        $env:PDFIUM_DYNAMIC_LIB_PATH = $inheritedOverride
        Restore-VisualCRuntime $hidden
    }

    $receipt = @{}
    # UTF-8 explicitly: the harness writes the resolved library path, and
    # Windows PowerShell otherwise reads the file in the console's ANSI
    # codepage, which mangles any non-ASCII character in the install path.
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
}
