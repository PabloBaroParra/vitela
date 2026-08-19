#Requires -Version 5.1
<#
.SYNOPSIS
    Authenticode-signs the binaries in a staged Windows package (T-064).

.DESCRIPTION
    Two modes, and they are not interchangeable:

      -PfxBase64/-PfxPassword   a real code-signing certificate, supplied by
                                the release secrets. Timestamped, so the
                                signature outlives the certificate.

      -DevelopmentCertificate   a throwaway self-signed certificate created for
                                this run and deleted afterwards. It exercises
                                the signing path on every pull request — where
                                secrets are unavailable, including every fork —
                                and produces something no machine will trust.
                                Development only, never distributable. This is
                                the same posture macos.yml holds with its
                                ad-hoc signed artifact while the real identity
                                is missing.

    Only the binaries this project produces or bundles are signed. Everything
    else in the package is Microsoft-signed already, and re-signing a valid
    signature would replace evidence with our own weaker claim.

    Set-AuthenticodeSignature rather than signtool.exe: it is Authenticode
    either way, needs no Windows SDK path hunting, and takes the same PFX. A
    hardware token or EV/HSM identity does need signtool — that swap belongs
    here, in one script, on the day the maintainer has such an identity.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$StageDir,
    [string]$PfxBase64,
    [string]$PfxPassword,
    [string]$TimestampServer = 'http://timestamp.digicert.com',
    [switch]$DevelopmentCertificate
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Fail([string]$message) { throw "sign-windows-binaries: $message" }

if (-not (Test-Path -LiteralPath $StageDir -PathType Container)) { Fail "staged package not found: $StageDir" }

# The app's own code plus the third-party library it bundles. PDFium ships
# unsigned from bblanchon/pdfium-binaries, and an unsigned DLL loaded next to a
# signed executable is exactly the gap the signature is supposed to close.
$targets = @(
    'Pdf.Windows.exe',
    'Pdf.Windows.dll',
    'pdf_ffi.dll',
    'pdfium.dll'
) | ForEach-Object {
    $path = Join-Path $StageDir $_
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { Fail "file to sign not found: $path" }
    $path
}

$temporaryCertificate = $null
$certificate = $null

if ($PfxBase64) {
    $pfxPath = Join-Path ([System.IO.Path]::GetTempPath()) ("vitela-signing-" + [guid]::NewGuid().ToString('n') + '.pfx')
    try {
        [System.IO.File]::WriteAllBytes($pfxPath, [System.Convert]::FromBase64String($PfxBase64))
        $securePassword = if ($PfxPassword) { ConvertTo-SecureString -String $PfxPassword -AsPlainText -Force } else { $null }
        # Loaded with an ephemeral key set: the certificate never lands in a
        # store this run does not own, and nothing survives the process.
        $certificate = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2(
            $pfxPath,
            $securePassword,
            [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::EphemeralKeySet)
    }
    finally {
        if (Test-Path -LiteralPath $pfxPath) { Remove-Item -LiteralPath $pfxPath -Force }
    }
    Write-Host "sign-windows-binaries: signing with the supplied certificate ($($certificate.Subject))"
}
elseif ($DevelopmentCertificate) {
    $temporaryCertificate = New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject 'CN=Vitela Development Build (not for distribution)' `
        -CertStoreLocation Cert:\CurrentUser\My `
        -KeyUsage DigitalSignature `
        -KeyExportPolicy NonExportable `
        -NotAfter (Get-Date).AddDays(1)
    $certificate = $temporaryCertificate
    Write-Host 'sign-windows-binaries: signing with a throwaway development certificate - NOT distributable'
}
else {
    Fail 'no signing identity: pass -PfxBase64 (release) or -DevelopmentCertificate (CI without secrets)'
}

try {
    foreach ($target in $targets) {
        $arguments = @{
            FilePath      = $target
            Certificate   = $certificate
            HashAlgorithm = 'SHA256'
        }
        # A self-signed certificate that expires tomorrow gains nothing from a
        # timestamp, and asking a public timestamp authority to vouch for it on
        # every pull request is traffic nobody needs.
        if (-not $DevelopmentCertificate -and $TimestampServer) {
            $arguments['TimestampServer'] = $TimestampServer
        }

        $result = Set-AuthenticodeSignature @arguments
        if ($result.Status -ne 'Valid' -and $result.Status -ne 'UnknownError') {
            Fail "signing $([System.IO.Path]::GetFileName($target)) failed: $($result.Status) $($result.StatusMessage)"
        }
        if (-not $result.SignerCertificate) {
            Fail "signing $([System.IO.Path]::GetFileName($target)) produced no signature"
        }
        Write-Host "sign-windows-binaries: signed $([System.IO.Path]::GetFileName($target)) ($($result.Status))"
    }
}
finally {
    if ($temporaryCertificate) {
        Remove-Item -LiteralPath "Cert:\CurrentUser\My\$($temporaryCertificate.Thumbprint)" -Force -ErrorAction SilentlyContinue
    }
}
