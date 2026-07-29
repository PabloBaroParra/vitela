namespace Pdf.Windows.Facade;

internal interface IPdfCoreDocument : IDisposable
{
    uint PageCount { get; }

    /// <summary>
    /// Per-page layout size in PDF points, in page order, from the same
    /// bytes rendering draws — placeholder sizes always match render output.
    /// </summary>
    IReadOnlyList<PdfCorePageDimensions> PageDimensions { get; }
}

internal sealed record PdfCorePageDimensions(double WidthPt, double HeightPt);

internal interface IPdfCore
{
    IPdfCoreDocument OpenFromBytes(byte[] bytes, string? password);

    IPdfCoreDocument CreateBlank();

    PdfCoreBitmap RenderPage(IPdfCoreDocument document, uint pageIndex, uint dpi, bool invertContentColors);

    PdfCoreBitmap RenderPageRegion(IPdfCoreDocument document, uint pageIndex, uint dpi, PageRegion region, bool invertContentColors);

    /// <summary>
    /// Renders every tile of one page in a single core call. The page is loaded
    /// — and its content stream parsed — once for the whole batch, which is
    /// what makes covering a deep-zoom viewport affordable.
    /// </summary>
    IReadOnlyList<PdfCoreBitmap> RenderPageTiles(IPdfCoreDocument document, uint pageIndex, uint dpi, IReadOnlyList<PageRegion> tiles, bool invertContentColors);

    IReadOnlyList<PdfCoreSearchHit> Search(IPdfCoreDocument document, string query);
}

internal sealed record PdfCoreBitmap(uint Width, uint Height, uint Stride, byte[] Rgba);
internal sealed record PdfCoreSearchRect(double XPt, double YPt, double WidthPt, double HeightPt);
internal sealed record PdfCoreSearchHit(uint PageIndex, string Text, IReadOnlyList<PdfCoreSearchRect> CharacterBounds);

/// <summary>
/// The renderer may pad each pixel row to a stride wider than width * 4;
/// UI consumers (BitmapEncoder.SetPixelData) require tightly packed rows.
/// </summary>
internal static class PdfBitmapRows
{
    public static byte[] TightlyPacked(byte[] pixels, uint width, uint height, uint stride)
    {
        var rowBytes = checked((int)width * 4);
        if (stride == rowBytes)
        {
            return pixels;
        }

        var strideBytes = checked((int)stride);
        var tight = new byte[checked(rowBytes * (int)height)];
        for (var row = 0; row < (int)height; row++)
        {
            Buffer.BlockCopy(pixels, row * strideBytes, tight, row * rowBytes, rowBytes);
        }

        return tight;
    }
}

internal enum PdfCoreError
{
    PasswordRequired,
    WrongPassword,
    UnsupportedSecurityHandler,
    DocumentNotFound,
    BitmapNotFound,
    PageIndexOutOfBounds,
    AnnotationNotFound,
    InvalidImage,
    InvalidSaveRequest,
    UnsupportedOperation,
    RenderFailed,
    Io,
    Internal
}

internal sealed class PdfCoreException : Exception
{
    public PdfCoreException(PdfCoreError category, string diagnosticDetail) : base(diagnosticDetail)
    {
        Category = category;
    }

    public PdfCoreError Category { get; }
}

internal interface IDiagnosticLogger
{
    void Failure(PdfCoreError category, string operation, string correlationId, string? sessionId, uint? pageIndex, string sanitizedDetail);
}

internal sealed class DebugDiagnosticLogger : IDiagnosticLogger
{
    public void Failure(PdfCoreError category, string operation, string correlationId, string? sessionId, uint? pageIndex, string sanitizedDetail)
    {
        System.Diagnostics.Debug.WriteLine($"PDF failure {category} {operation} {correlationId} {sessionId} {pageIndex} {sanitizedDetail}");
    }
}
