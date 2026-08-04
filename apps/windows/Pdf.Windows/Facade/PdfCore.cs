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

    /// <summary>
    /// Renders every tile of one page in a single core call. The page is loaded
    /// — and its content stream parsed — once for the whole batch, which is
    /// what makes covering a deep-zoom viewport affordable. A single tile is
    /// this call with a one-element <paramref name="tiles"/>.
    /// </summary>
    IReadOnlyList<PdfCoreBitmap> RenderPageTiles(IPdfCoreDocument document, uint pageIndex, uint dpi, IReadOnlyList<PageRegion> tiles, bool invertContentColors);

    IReadOnlyList<PdfCoreSearchHit> Search(IPdfCoreDocument document, string query);

    IReadOnlyList<PdfCoreAnnotation> Annotations(IPdfCoreDocument document);

    bool AnnotationEditingAllowed(IPdfCoreDocument document);

    bool CanUndo(IPdfCoreDocument document);

    bool CanRedo(IPdfCoreDocument document);

    void ApplyEdit(IPdfCoreDocument document, PdfCoreEdit edit);

    /// <summary>
    /// Inserts a Stamp annotation from raw image bytes — separate from
    /// <see cref="ApplyEdit"/> because it is the one annotation edit that does
    /// not fit <see cref="PdfCoreEdit"/>'s value-type shape (image bytes, not a
    /// small struct).
    /// </summary>
    void InsertImageStamp(IPdfCoreDocument document, uint pageIndex, byte[] imageBytes, PdfCoreRect rect);

    bool Undo(IPdfCoreDocument document);

    bool Redo(IPdfCoreDocument document);

    byte[] SaveToBytes(IPdfCoreDocument document);
}

internal sealed record PdfCoreBitmap(uint Width, uint Height, uint Stride, byte[] Rgba);
internal sealed record PdfCoreSearchRect(double XPt, double YPt, double WidthPt, double HeightPt);
internal sealed record PdfCoreSearchHit(uint PageIndex, string Text, IReadOnlyList<PdfCoreSearchRect> CharacterBounds);
internal sealed record PdfCoreRect(double X, double Y, double Width, double Height);
internal sealed record PdfCoreColor(byte R, byte G, byte B);
internal sealed record PdfCorePoint(double X, double Y);
internal enum PdfCoreAnnotationKind { Highlight, Underline, Strikeout, Ink, Shape, TextNote, Stamp }
internal sealed record PdfCoreAnnotation(ulong Id, uint PageIndex, PdfCoreAnnotationKind Kind, PdfCoreRect? Rect, PdfCoreColor? Color, IReadOnlyList<PdfCorePoint> Points);
internal abstract record PdfCoreEdit
{
    public sealed record Add(PdfCoreAnnotationKind Kind, uint PageIndex, PdfCoreRect Rect, PdfCoreColor Color, IReadOnlyList<PdfCorePoint>? Points = null, string? Contents = null) : PdfCoreEdit;
    public sealed record Remove(ulong AnnotationId) : PdfCoreEdit;
    public sealed record Move(ulong AnnotationId, double Dx, double Dy) : PdfCoreEdit;
    public sealed record Resize(ulong AnnotationId, PdfCoreRect Rect) : PdfCoreEdit;
    public sealed record Restyle(ulong AnnotationId, PdfCoreColor Color) : PdfCoreEdit;
}

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
    UnsavedChanges,
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

