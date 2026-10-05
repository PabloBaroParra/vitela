using uniffi.pdf_ffi;

namespace Pdf.Windows.Facade;

internal sealed partial class GeneratedPdfCore
{
    public string? SigningRefusal(IPdfCoreDocument document) =>
        PdfFfiMethods.SigningRefusal(((GeneratedDocument)document).Handle);

    public ISigningCertificate OpenSigningCertificate(byte[] bytes, string password)
    {
        try { return new Certificate(PdfFfiMethods.OpenSigningCertificate(bytes, password)); }
        catch (FfiException error) { throw Translate(error); }
    }

    public byte[] SignToBytes(IPdfCoreDocument document, ISigningCertificate certificate, string identityId)
    {
        try { return PdfFfiMethods.SignToBytes(((GeneratedDocument)document).Handle, ((Certificate)certificate).Handle, identityId); }
        catch (FfiException error) { throw Translate(error); }
    }

    public IPdfCoreDocument ReopenSignedDocument(IPdfCoreDocument source, byte[] bytes)
    {
        try { return new GeneratedDocument(PdfFfiMethods.ReopenSignedDocument(((GeneratedDocument)source).Handle, bytes)); }
        catch (FfiException error) { throw Translate(error); }
    }

    public ISigningCertificate OpenTokenSigningSource(string modulePath, string? pin)
    {
        try { return new Certificate(PdfFfiMethods.OpenTokenSigningSource(modulePath, pin)); }
        catch (FfiException error) { throw Translate(error); }
    }

    public ISigningCertificate OpenSystemSigningSource()
    {
        var source = WindowsSigningStore.Open();
        try { return new Certificate(PdfFfiMethods.OpenPlatformSigningSource(source), source); }
        catch (FfiException error) { source.Dispose(); throw Translate(error); }
        catch { source.Dispose(); throw; }
    }

    private sealed class Certificate(SigningCertificate handle, IDisposable? owner = null) : ISigningCertificate
    {
        internal SigningCertificate Handle { get; } = handle;
        public IReadOnlyList<SigningIdentity> Identities => Handle.Identities().Select(identity => new SigningIdentity(identity.Id, identity.DisplayName)).ToArray();
        public void Dispose() { Handle.Dispose(); owner?.Dispose(); }
    }
}
