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

    /// <summary>
    /// Loads and flattens one page's characters for caret hit-testing and
    /// selection-rect queries — the geometry a drag-select needs on every
    /// pointer-move. Callers should hold the returned handle for the life of
    /// one drag (or the page's lifetime) rather than reloading it per move.
    /// </summary>
    IPdfCorePageCharacters PageCharacters(IPdfCoreDocument document, uint pageIndex);

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

    /// <summary>
    /// The rect a dropped or pasted image gets, sized from the image's own
    /// proportions. Takes no document because it is pure geometry.
    ///
    /// The shell asks the core rather than computing it, so this shell and the
    /// GTK one cannot disagree about where the same image lands — the size
    /// policy has exactly one home, in <c>pdf_annotate::placement</c>.
    /// </summary>
    PdfCoreRect StampPlacement(byte[] imageBytes, double anchorX, double anchorY);

    bool Undo(IPdfCoreDocument document);

    bool Redo(IPdfCoreDocument document);

    /// <summary>
    /// Whether saving <paramref name="document"/> would break a signature the
    /// file already carries.
    /// </summary>
    /// <remarks>
    /// A save that rewrites the file cannot preserve a signature, and page
    /// content editing makes a rewrite reachable from an ordinary text change.
    /// The core refuses such a save unless the caller states the user was told
    /// (see <see cref="SaveToBytes"/>), so this is how the shell finds out
    /// there is something to tell them.
    /// </remarks>
    bool WillInvalidateSignatures(IPdfCoreDocument document);

    /// <summary>
    /// Saves <paramref name="document"/>.
    /// </summary>
    /// <param name="signaturesAcknowledged">
    /// Pass <c>true</c> only once the user has been told the save will break
    /// an existing signature and chose to continue. With <c>false</c>, such a
    /// save fails with <see cref="PdfCoreError.SignaturesWouldBeInvalidated"/>
    /// rather than silently producing a file whose signature no longer
    /// verifies.
    /// </param>
    byte[] SaveToBytes(IPdfCoreDocument document, bool signaturesAcknowledged);
}

/// <summary>
/// One page's characters, flattened for repeated caret/selection queries by
/// the shell — mirrors <c>pdf_render::PageCharacters</c>, the geometry both
/// this shell and the GTK one build a drag-select from (the GTK shell links
/// it directly; this shell reaches it through the FFI's <c>FfiPageCharacters</c>).
/// </summary>
internal interface IPdfCorePageCharacters : IDisposable
{
    /// <summary>
    /// The caret nearest a PDF-space point (bottom-left origin), or
    /// <c>null</c> on a page with no positioned text.
    /// </summary>
    uint? CaretAt(double xPt, double yPt);

    /// <summary>
    /// The text between two carets, for the clipboard. Order does not
    /// matter — a drag started rightward or leftward reports the same text.
    /// </summary>
    string TextIn(uint anchor, uint focus);

    /// <summary>The rects a shell paints between two carets: one per visual line, not one per glyph.</summary>
    IReadOnlyList<PdfCoreSearchRect> RectsIn(uint anchor, uint focus);
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
    SignaturesWouldBeInvalidated,
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

