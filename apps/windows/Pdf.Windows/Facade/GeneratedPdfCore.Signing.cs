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

    private sealed class Certificate(SigningCertificate handle) : ISigningCertificate
    {
        internal SigningCertificate Handle { get; } = handle;
        public IReadOnlyList<SigningIdentity> Identities => Handle.Identities().Select(identity => new SigningIdentity(identity.Id, identity.DisplayName)).ToArray();
        public void Dispose() => Handle.Dispose();
    }
}
