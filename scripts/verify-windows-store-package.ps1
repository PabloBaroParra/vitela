#Requires -Version 5.1
<#
.SYNOPSIS
    Verifies the Microsoft Store MSIX by running it: an offline render with its
    own native files, and a real install and launch.

.DESCRIPTION
    package-windows-store.ps1 checks what the package contains. This script
    checks that it works. Both checks exist because 0.1.101.0 passed every
    static check, and Store certification then found "Open PDF" unusable on a
    clean laptop: pdf_ffi.dll needed a Visual C++ runtime that only the build
    machines had.

      1. Render. The .msix is extracted, and its pdf_ffi.dll and pdfium.dll
         render the sample through Pdf.Windows.PackageSmoke
         (scripts/windows-package-smoke.ps1).
      2. -CleanMachine. The render runs with the Visual C++ runtime hidden from
         the runner, as on a machine that never had Visual Studio.
      3. -InstallAndLaunch. A copy of the .msix is signed with a throwaway
         certificate whose subject is the package's Publisher, its Windows App
         Runtime dependency is installed, and the app is installed and
         activated by its package identity. It must still be running a while
         later. This catches what the render cannot: a manifest, asset or
         packaged-startup failure. The file under -PackagesDir is never
         signed or changed, because the Store signs what it receives.

    2 and 3 change the machine (System32, the certificate trust store, the
    installed packages) and run only on a GitHub-hosted runner.
#>
[CmdletBinding()]
param(
    [string]$PackagesDir,
    [string]$EvidenceDir,
    [string]$SmokeProject,
    [switch]$CleanMachine,
    [switch]$InstallAndLaunch,
    # How long the launched app must stay alive.
    [int]$AliveSeconds = 20
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Fail([string]$message) { throw "verify-windows-store-package: $message" }

. (Join-Path $PSScriptRoot 'windows-package-smoke.ps1')

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$buildRoot = Join-Path $repositoryRoot 'build\windows-store'
if (-not $PackagesDir) { $PackagesDir = Join-Path $buildRoot 'packages' }
if (-not $EvidenceDir) { $EvidenceDir = Join-Path $buildRoot 'evidence' }
if (-not $SmokeProject) {
    $SmokeProject = Join-Path $repositoryRoot 'apps\windows\Pdf.Windows.PackageSmoke\Pdf.Windows.PackageSmoke.csproj'
}

# The same selection as package-windows-store.ps1: one .msix, not the Windows
# App SDK runtime packages under Dependencies\.
$msix = @(Get-ChildItem -LiteralPath $PackagesDir -Recurse -File |
    Where-Object { $_.Extension -eq '.msix' -and $_.FullName -notlike '*\Dependencies\*' })
if ($msix.Count -ne 1) { Fail "expected one .msix under $PackagesDir, found $($msix.Count)" }
$msix = $msix[0]

if (Test-Path -LiteralPath $EvidenceDir) { Remove-Item -LiteralPath $EvidenceDir -Recurse -Force }
$layoutDir = Join-Path $EvidenceDir 'layout'
New-Item -ItemType Directory -Force -Path $layoutDir | Out-Null
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::ExtractToDirectory($msix.FullName, $layoutDir)

$manifest = [xml](Get-Content -LiteralPath (Join-Path $layoutDir 'AppxManifest.xml') -Raw)
$identity = $manifest.Package.Identity
$applicationId = @($manifest.Package.Applications.Application)[0].Id
"package=$($msix.Name)", "identity=$($identity.Name)", "version=$($identity.Version)", "publisher=$($identity.Publisher)" |
    Set-Content -LiteralPath (Join-Path $EvidenceDir 'identity.txt')

# 1 and 2: the packaged native files render a page.
Invoke-PackageSmoke -InstallDir $layoutDir -EvidenceDir $EvidenceDir -SmokeProject $SmokeProject -CleanMachine:$CleanMachine
Write-Host "verify-windows-store-package: rendered the sample with the packaged files$(if ($CleanMachine) { ' (Visual C++ runtime hidden)' })"

if (-not $InstallAndLaunch) {
    'verified Store MSIX render' | Set-Content -LiteralPath (Join-Path $EvidenceDir 'result.txt')
    return
}

# 3: install and launch.
Assert-DisposableRunner 'installing and launching the package'

$signtool = Get-ChildItem -LiteralPath "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter signtool.exe -File |
    Where-Object { $_.FullName -like '*\x64\*' } | Sort-Object FullName -Descending | Select-Object -First 1
if (-not $signtool) { Fail 'signtool.exe not found in the Windows SDK' }

# Install accepts a package only when its signer matches Publisher, so the
# test certificate takes the package's subject. Code-signing EKU, no CA.
$certificate = New-SelfSignedCertificate -Type Custom -Subject $identity.Publisher -KeyUsage DigitalSignature `
    -FriendlyName 'Vitela CI install test' -CertStoreLocation 'Cert:\CurrentUser\My' `
    -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
$pfxPath = Join-Path $EvidenceDir 'install-test.pfx'
$pfxPassword = [Guid]::NewGuid().ToString('N')
Export-PfxCertificate -Cert $certificate -FilePath $pfxPath -Password (ConvertTo-SecureString $pfxPassword -AsPlainText -Force) | Out-Null
$cerPath = Join-Path $EvidenceDir 'install-test.cer'
Export-Certificate -Cert $certificate -FilePath $cerPath | Out-Null
Import-Certificate -FilePath $cerPath -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople' | Out-Null

$signedCopy = Join-Path $EvidenceDir $msix.Name
Copy-Item -LiteralPath $msix.FullName -Destination $signedCopy
& $signtool.FullName sign /fd SHA256 /f $pfxPath /p $pfxPassword $signedCopy | Out-Null
if ($LASTEXITCODE -ne 0) { Fail 'signtool could not sign the install-test copy' }
Remove-Item -LiteralPath $pfxPath

# The Store installs the framework dependency itself. Here it comes from the
# runtime packages the build left under Dependencies\x64.
$runtimeDependency = $manifest.Package.Dependencies.PackageDependency | Where-Object { $_.Name -like 'Microsoft.WindowsAppRuntime.*' } | Select-Object -First 1
foreach ($dependency in @(Get-ChildItem -LiteralPath $msix.DirectoryName -Recurse -File -Filter '*.msix' |
            Where-Object { $_.FullName -like '*\Dependencies\x64\*' })) {
    try { Add-AppxPackage -Path $dependency.FullName }
    catch {
        # A newer copy already on the runner satisfies MinVersion, and
        # Add-AppxPackage refuses the older one.
        if (-not (Get-AppxPackage -Name $runtimeDependency.Name | Where-Object { [version]$_.Version -ge [version]$runtimeDependency.MinVersion })) { throw }
    }
}

Add-AppxPackage -Path $signedCopy
$installed = Get-AppxPackage -Name $identity.Name | Where-Object { $_.Version -eq $identity.Version }
if (-not $installed) { Fail "$($identity.Name) $($identity.Version) is not installed after Add-AppxPackage" }
$aumid = "$($installed.PackageFamilyName)!$applicationId"

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace VitelaCi {
    [ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IApplicationActivationManager {
        [PreserveSig] int ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string appUserModelId, [MarshalAs(UnmanagedType.LPWStr)] string arguments, int options, out uint processId);
    }
    [ComImport, Guid("45BA127D-10A8-46EA-8AB7-56EA9078943C")]
    class ApplicationActivationManager { }
    public static class Activation {
        public static uint Activate(string aumid) {
            uint pid;
            int hr = ((IApplicationActivationManager)new ApplicationActivationManager()).ActivateApplication(aumid, null, 0, out pid);
            Marshal.ThrowExceptionForHR(hr);
            return pid;
        }
    }
}
'@

$hidden = @()
$launchReport = Join-Path $EvidenceDir 'launch.txt'
try {
    # The app runs on the same stripped machine as the render.
    if ($CleanMachine) { $hidden = Hide-VisualCRuntime }
    $started = Get-Date
    $processId = [VitelaCi.Activation]::Activate($aumid)
    Start-Sleep -Seconds $AliveSeconds
    $process = Get-Process -Id $processId -ErrorAction SilentlyContinue
    if (-not $process) {
        $crashes = Get-WinEvent -FilterHashtable @{ LogName = 'Application'; StartTime = $started } -ErrorAction SilentlyContinue |
            Where-Object { $_.Message -like '*Pdf.Windows*' } | ForEach-Object { "[$($_.ProviderName) $($_.Id)] $($_.Message)" }
        $crashes | Set-Content -LiteralPath $launchReport
        Fail "the installed app exited within $AliveSeconds s of launch (see $launchReport)"
    }
    if (-not $process.Path -or -not $process.Path.StartsWith($installed.InstallLocation, [StringComparison]::OrdinalIgnoreCase)) {
        Fail "the activated process runs $($process.Path), not the installed package at $($installed.InstallLocation)"
    }
    "aumid=$aumid", "pid=$processId", "path=$($process.Path)", "alive_seconds=$AliveSeconds" | Set-Content -LiteralPath $launchReport
}
finally {
    Get-Process -Name Pdf.Windows -ErrorAction SilentlyContinue | Stop-Process -Force
    Restore-VisualCRuntime $hidden
    Get-AppxPackage -Name $identity.Name | Remove-AppxPackage -ErrorAction SilentlyContinue
}

'verified Store MSIX render, install and launch' | Set-Content -LiteralPath (Join-Path $EvidenceDir 'result.txt')
Write-Host "verify-windows-store-package: installed and launched $aumid; alive after $AliveSeconds s"
