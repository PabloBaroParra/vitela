using System.Security.Cryptography;
using System.Security.Cryptography.X509Certificates;
using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in native entry point. Generates an ephemeral test identity, never reads a user's key.
public partial class App : Application
{
    private MainWindow? _window;
    public App() => BundledPdfium.PointCoreAtBundledLibrary();
    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var output = Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")
            ?? throw new InvalidOperationException("Set VITELA_SMOKE_OUTPUT to an existing directory.");
        _window = new MainWindow();
        _window.Activate();
        try
        {
            await _window.SigningSmokeAsync();
            File.WriteAllText(Path.Combine(output, "signing-smoke.log"), "PASS signing source gates; wrong-password retry; real PFX identity; signed save/reopen; second signature; preserved encrypted source; Windows RSA/ECDSA digest verification and native callback encrypted sign/reopen; current-user store discovery; invalid token-module refusal.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "signing-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task SigningSmokeAsync()
    {
        static void Check(bool value, string message)
        {
            if (!value) throw new InvalidOperationException(message);
        }
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        Check(Content.XamlRoot is not null, "Window did not load");
        Check(!_chooseSigningCertificate.IsEnabled && !_chooseCardCertificate.IsEnabled && !_chooseComputerCertificate.IsEnabled && _signedIndicator.Visibility == Visibility.Collapsed, "No-document controls");
        await OpenDocumentAsync("Signing smoke", await File.ReadAllBytesAsync(SamplePath));
        await RefreshSigningStateAsync();
        SelectToolPage("Sign");
        Check(_chooseSigningCertificate.IsEnabled && _signedIndicator.Visibility == Visibility.Collapsed, "Unsigned sample controls");
        SetBusy(true);
        Check(!_chooseSigningCertificate.IsEnabled && !_chooseCardCertificate.IsEnabled && !_chooseComputerCertificate.IsEnabled, "Busy controls");
        SetBusy(false);
        Check(_chooseSigningCertificate.IsEnabled, "Busy recovery");
        var original = _signingState!;
        _signingState = original with { Refusal = "permission refused" };
        UpdateSigningControls();
        Check(!_chooseSigningCertificate.IsEnabled, "Permission controls");
        _signingState = original with { SessionId = "stale" };
        UpdateSigningControls();
        Check(!_chooseSigningCertificate.IsEnabled, "Stale snapshot controls");
        _signingState = original;
        UpdateSigningControls();

        using var rsa = RSA.Create(2048);
        var request = new CertificateRequest("CN=Vitela ephemeral signing smoke", rsa, HashAlgorithmName.SHA256, RSASignaturePadding.Pkcs1);
        using var testCertificate = request.CreateSelfSigned(DateTimeOffset.UtcNow.AddDays(-1), DateTimeOffset.UtcNow.AddDays(1));
        var pfx = testCertificate.Export(X509ContentType.Pfx, "smoke-only");
        try
        {
            var wrong = await _facade.OpenSigningCertificateAsync(pfx, "wrong");
            Check(!wrong.IsSuccess, "Wrong password must fail");
            var unlocked = await _facade.OpenSigningCertificateAsync(pfx, "smoke-only");
            Check(unlocked.IsSuccess, "Real PFX unlock: " + unlocked.Error?.Message);
            using var certificate = unlocked.Value!;
            var identities = certificate.Identities;
            Check(identities.Count == 1 && identities[0].DisplayName.Contains("Vitela"), "Real identity mapping");
            for (var index = 0; index < 2; index++)
            {
                var result = await _facade.SignToDestinationAsync(_session!.SessionId, certificate, identities[0].Id, "signed-smoke.pdf", bytes =>
                {
                    Check(bytes.Length > 100 && System.Text.Encoding.ASCII.GetString(bytes).Contains("/ByteRange"), "Signed output lacks signature");
                    return Task.CompletedTask;
                });
                Check(result.IsSuccess, "Real signature/reopen: " + result.Error?.Message);
                ShowOpenedDocument(result.Value!);
                await RefreshSigningStateAsync();
                Check(_signedIndicator.Visibility == Visibility.Visible && _chooseSigningCertificate.IsEnabled, "Signed indicator/re-sign eligibility");
            }

            var blank = await _facade.CreateBlankAsync(true);
            Check(blank.IsSuccess, "Blank test source");
            byte[]? protectedBytes = null;
            var protectedResult = await _facade.ProtectToDestinationAsync(blank.Value!.SessionId, "open-test", "owner-test", bytes => { protectedBytes = bytes; return Task.CompletedTask; });
            Check(protectedResult.IsSuccess && protectedBytes is not null, "Encrypted test source");
            var encrypted = await _facade.ReopenProtectedAsync(blank.Value.SessionId, "encrypted-smoke.pdf", protectedBytes!, "open-test", "owner-test");
            Check(encrypted.IsSuccess, "Encrypted reopen");
            var encryptedSigned = await _facade.SignToDestinationAsync(encrypted.Value!.SessionId, certificate, identities[0].Id, "encrypted-signed.pdf", signedBytes =>
            {
                try { using var unexpected = uniffi.pdf_ffi.PdfFfiMethods.OpenFromBytes(signedBytes, null); throw new InvalidOperationException("Signed output lost encryption"); }
                catch (uniffi.pdf_ffi.FfiException.PasswordRequired) { }
                using var userOpened = uniffi.pdf_ffi.PdfFfiMethods.OpenFromBytes(signedBytes, "open-test");
                Check(userOpened.PageCount() == 1, "User password no longer opens signed output");
                return Task.CompletedTask;
            });
            Check(encryptedSigned.IsSuccess, "Encrypted signature/reopen: " + encryptedSigned.Error?.Message);
            ShowOpenedDocument(encryptedSigned.Value!);
            await RefreshSigningStateAsync();
            Check(_signedIndicator.Visibility == Visibility.Visible, "Encrypted signed indicator");

            // Exercise the real Windows provider callback without reading or modifying a user's private keys/store.
            using var ec = ECDsa.Create(ECCurve.NamedCurves.nistP256);
            var ecRequest = new CertificateRequest("CN=Vitela ephemeral ECDSA store smoke", ec, HashAlgorithmName.SHA256);
            using var ecCertificate = ecRequest.CreateSelfSigned(DateTimeOffset.UtcNow.AddDays(-1), DateTimeOffset.UtcNow.AddDays(1));
            using var storeSource = WindowsSigningStore.FromCertificates([new X509Certificate2(testCertificate), new X509Certificate2(ecCertificate)]);
            var digest = SHA256.HashData("native-store-smoke"u8);
            Check(rsa.VerifyHash(digest, storeSource.SignDigest(testCertificate.Thumbprint, digest, uniffi.pdf_ffi.FfiSigningAlgorithm.RsaSha256), HashAlgorithmName.SHA256, RSASignaturePadding.Pkcs1), "Native RSA digest signature");
            Check(ec.VerifyHash(digest, storeSource.SignDigest(ecCertificate.Thumbprint, digest, uniffi.pdf_ffi.FfiSigningAlgorithm.EcdsaSha256), DSASignatureFormat.Rfc3279DerSequence), "Native ECDSA DER digest signature");
            using var storeHandle = uniffi.pdf_ffi.PdfFfiMethods.OpenPlatformSigningSource(storeSource);
            using var protectedHandle = uniffi.pdf_ffi.PdfFfiMethods.OpenWithPasswordsFromBytes(protectedBytes!, "open-test", "owner-test");
            foreach (var identity in storeHandle.Identities())
            {
                var signed = uniffi.pdf_ffi.PdfFfiMethods.SignToBytes(protectedHandle, storeHandle, identity.Id);
                using var reopened = uniffi.pdf_ffi.PdfFfiMethods.ReopenSignedDocument(protectedHandle, signed);
                Check(reopened.PageCount() == 1 && uniffi.pdf_ffi.PdfFfiMethods.ExtractSourceIsSigned(reopened), "Native store callback encrypted signing/reopen");
            }
            var systemSource = await _facade.OpenSystemSigningSourceAsync();
            Check(systemSource.IsSuccess, "Current-user store discovery: " + systemSource.Error?.Message);
            systemSource.Value!.Dispose();
            var invalidToken = await _facade.OpenTokenSigningSourceAsync(Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString("N") + ".dll"), null);
            Check(!invalidToken.IsSuccess, "Invalid token module must be refused");
        }
        finally { Array.Clear(pfx); }
    }
}
