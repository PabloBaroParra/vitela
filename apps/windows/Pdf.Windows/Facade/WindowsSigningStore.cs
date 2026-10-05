using System.Security.Cryptography;
using System.Security.Cryptography.X509Certificates;
using uniffi.pdf_ffi;

namespace Pdf.Windows.Facade;

/// <summary>Current user's personal store. Keys remain in Windows providers; only public DER and signatures cross FFI.</summary>
internal sealed class WindowsSigningStore : PlatformSigningSource, IDisposable
{
    private readonly Dictionary<string, X509Certificate2> _certificates = new(StringComparer.Ordinal);
    private readonly List<FfiPlatformSigningIdentity> _identities = [];

    internal static WindowsSigningStore Open()
    {
        using var store = new X509Store(StoreName.My, StoreLocation.CurrentUser);
        store.Open(OpenFlags.ReadOnly | OpenFlags.OpenExistingOnly);
        return FromCertificates(store.Certificates);
    }

    // Takes ownership of the enumeration's certificate handles, including on failure.
    internal static WindowsSigningStore FromCertificates(IEnumerable<X509Certificate2> certificates)
    {
        var owned = certificates.ToArray();
        var source = new WindowsSigningStore();
        try
        {
            foreach (var certificate in owned)
            {
                if (!source.Add(certificate)) certificate.Dispose();
            }
            return source;
        }
        catch
        {
            source.Dispose();
            foreach (var certificate in owned) certificate.Dispose();
            throw;
        }
    }

    private bool Add(X509Certificate2 certificate)
    {
        if (!certificate.HasPrivateKey) return false;
        var algorithm = certificate.PublicKey.Oid.Value switch
        {
            "1.2.840.113549.1.1.1" => FfiSigningAlgorithm.RsaSha256,
            "1.2.840.10045.2.1" => FfiSigningAlgorithm.EcdsaSha256,
            _ => (FfiSigningAlgorithm?)null,
        };
        if (algorithm is null) return false;
        var id = certificate.Thumbprint;
        if (!_certificates.TryAdd(id, certificate)) return false;
        var chainBytes = new List<byte[]> { certificate.RawData };
        using var chain = new X509Chain();
        chain.ChainPolicy.DisableCertificateDownloads = true;
        chain.ChainPolicy.RevocationMode = X509RevocationMode.NoCheck;
        // Discovery is not trust validation. Build offline solely to embed available public chain certificates.
        chain.Build(certificate);
        foreach (var element in chain.ChainElements.Cast<X509ChainElement>().Skip(1))
            chainBytes.Add(element.Certificate.RawData);
        _identities.Add(new FfiPlatformSigningIdentity(id, certificate.Subject, chainBytes.ToArray(), algorithm.Value));
        return true;
    }

    public FfiPlatformSigningIdentity[] Identities() => _identities.ToArray();

    public byte[] SignDigest(string identityId, byte[] digest, FfiSigningAlgorithm algorithm)
    {
        if (digest.Length != 32 || !_certificates.TryGetValue(identityId, out var certificate))
            throw new FfiException.UnsupportedOperation("The signing identity is no longer available.");
        try
        {
            switch (algorithm)
            {
                case FfiSigningAlgorithm.RsaSha256:
                    using (var rsa = certificate.GetRSAPrivateKey())
                        return rsa?.SignHash(digest, HashAlgorithmName.SHA256, RSASignaturePadding.Pkcs1)
                            ?? throw new CryptographicException("RSA key unavailable");
                case FfiSigningAlgorithm.EcdsaSha256:
                    using (var ecdsa = certificate.GetECDsaPrivateKey())
                        return ecdsa?.SignHash(digest, DSASignatureFormat.Rfc3279DerSequence)
                            ?? throw new CryptographicException("ECDSA key unavailable");
                default: throw new CryptographicException("Unsupported signing algorithm");
            }
        }
        catch (CryptographicException)
        {
            throw new FfiException.UnsupportedOperation("Windows could not use this signing key, or authentication was cancelled.");
        }
    }

    public void Dispose()
    {
        foreach (var certificate in _certificates.Values) certificate.Dispose();
        _certificates.Clear();
        _identities.Clear();
    }
}
