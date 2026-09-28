namespace Pdf.Windows.Facade;

internal interface IPdfCoreDocument : IDisposable
{
    uint PageCount { get; }

    /// <summary>
    /// Per-page layout size in PDF points, in the document's current page
    /// order — moves, removals and quarter-turns included, which is what the
    /// preview draws once it has been refreshed.
    /// </summary>
    IReadOnlyList<PdfCorePageDimensions> PageDimensions { get; }
}

internal sealed record PdfCorePageDimensions(double WidthPt, double HeightPt, PageRotation Rotation);

internal interface IPdfCore
{
    IPdfCoreDocument OpenFromBytes(byte[] bytes, string? password);

    IPdfCoreDocument OpenWithPasswordsFromBytes(byte[] bytes, string openPassword, string permissionsPassword);

    /// <summary>
    /// Returns a new document that already holds one page, ready to annotate.
    /// A zero-page result is not an acceptable implementation of this method —
    /// the reader has no way to add the first page.
    /// </summary>
    /// <remarks>
    /// Stated here because the facade tests cannot enforce it: this project
    /// compiles the facade against fakes only, never <c>GeneratedPdfCore</c>,
    /// so the guard for the real core call is the <c>pdf-ffi</c> smoke test
    /// <c>creates_a_document_that_already_has_one_renderable_page</c>.
    /// </remarks>
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

    PdfCoreDocumentInfo ReadDocumentInfo(IPdfCoreDocument document);

    /// <summary>
    /// Every AcroForm field the document holds, the ones the file already had
    /// included. Read-only, so it answers however restricted the document is;
    /// whether a field may be <em>filled</em> is
    /// <see cref="AnnotationEditingAllowed"/>'s answer, the core's floor for
    /// every field command.
    /// </summary>
    IReadOnlyList<PdfCoreFormField> ListFormFields(IPdfCoreDocument document);

    /// <summary>
    /// Parses one page's content stream and returns the text runs and images
    /// it paints — the editable page itself, not the annotations drawn over
    /// it. Parsed on demand and never cached by the core, so this is a real
    /// read every time.
    /// </summary>
    /// <remarks>
    /// The ids it hands back are only meaningful against the exact bytes the
    /// document was opened from. They survive <see cref="RefreshPreview"/>,
    /// which leaves those bytes alone, but not a save-and-reopen.
    /// </remarks>
    PdfCorePageContent ReadPageContent(IPdfCoreDocument document, uint pageIndex);

    /// <summary>
    /// The <c>/BaseFont</c> name of each font a page declares, keyed by the
    /// resource name its text runs report. Empty when the page names none.
    /// </summary>
    /// <remarks>
    /// A run says which resource paints it, not what that resource is — and a
    /// shell drawing its own editing overlay needs the second question
    /// answered, or it draws in a face the page does not use.
    /// </remarks>
    IReadOnlyDictionary<string, string> PageFontFamilies(IPdfCoreDocument document, uint pageIndex);

    /// <summary>
    /// Re-derives what <see cref="RenderPage"/> draws from the edits queued so
    /// far, so a retyped text run is visible before anything is saved.
    /// Retyped text <em>is</em> the page: unlike an annotation, no shell-side
    /// overlay can show it. Leaves the edit log and the bytes a save is
    /// computed from untouched, and deliberately excludes this session's own
    /// annotations, which the shell is already drawing itself.
    /// </summary>
    void RefreshPreview(IPdfCoreDocument document);

    bool AnnotationEditingAllowed(IPdfCoreDocument document);

    /// <summary>
    /// Whether this document permits rewriting its page content. Gated by a
    /// different <c>/P</c> bit than <see cref="AnnotationEditingAllowed"/>: a
    /// document can allow one and refuse the other, so neither answer may
    /// stand in for the other.
    /// </summary>
    bool ContentEditingAllowed(IPdfCoreDocument document);

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

    /// <summary>
    /// Where a page-space rect (points, bottom-left origin, unrotated) lands
    /// on the page as drawn. Pure geometry: the per-turn arithmetic lives in
    /// <c>pdf_render::selection</c>, the same code the GTK shell calls.
    /// </summary>
    PlacedRect PlaceRect(AnnotationRect rect, PagePlacement page);

    /// <summary><see cref="PlaceRect"/> for a bare point.</summary>
    PlacedPoint PlacePoint(AnnotationPoint point, PagePlacement page);

    /// <summary>The inverse of <see cref="PlacePoint"/>: a pointer position back into page space.</summary>
    AnnotationPoint PointToPdf(PlacedPoint point, PagePlacement page);

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

    bool ProtectionWillInvalidateSignatures(IPdfCoreDocument document);

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

    byte[] ProtectToBytes(IPdfCoreDocument document, string openPassword, string permissionsPassword, bool signaturesAcknowledged);

    /// <summary>
    /// Why <paramref name="document"/> cannot be compressed at all, in the
    /// core's own words, or <c>null</c> when it can.
    /// </summary>
    /// <remarks>
    /// Cheap — it reads the document's protection and saves nothing — so the
    /// shell asks it before offering a preset rather than running a
    /// compression that was never going to do anything.
    /// </remarks>
    string? CompressionRefusal(IPdfCoreDocument document);

    /// <summary>
    /// Whether compressing <paramref name="document"/> breaks a signature it
    /// already carries.
    /// </summary>
    /// <remarks>
    /// Deliberately not <see cref="WillInvalidateSignatures"/>: a compressed
    /// save is a full rewrite even with nothing edited, so a signed, unedited
    /// file answers <c>true</c> here and <c>false</c> there.
    /// </remarks>
    bool CompressedSaveWillInvalidateSignatures(IPdfCoreDocument document);

    /// <summary>
    /// Saves <paramref name="document"/> and compresses the result as far as
    /// <paramref name="preset"/> allows. Not an edit: no undo step, and the
    /// pending edits are left exactly as they were.
    /// </summary>
    PdfCoreCompressedSave SaveCompressedToBytes(IPdfCoreDocument document, PdfCoreCompressPreset preset, bool signaturesAcknowledged);

    /// <summary>
    /// Whether the document lets its text and graphics be extracted — the
    /// <c>/P</c> bit a page image needs, the same one search and copy ask.
    /// </summary>
    bool TextExtractionAllowed(IPdfCoreDocument document);

    /// <summary>
    /// Reads a one-based page range such as <c>"1-3,7"</c> into ascending,
    /// deduplicated zero-based positions. Takes no document: the grammar is
    /// the core's, shared with every shell, so <c>"7-3"</c> means the same
    /// thing everywhere.
    /// </summary>
    /// <exception cref="PdfCoreException">
    /// <see cref="PdfCoreError.InvalidPageSelection"/>, with the core's
    /// sentence for the reader in <see cref="PdfCoreException.ReaderFacingDetail"/>.
    /// </exception>
    IReadOnlyList<uint> ParsePageSelection(string input, uint totalPages);

    /// <summary>
    /// The file one exported page is written under — always a single path
    /// component, whatever <paramref name="documentName"/> holds.
    /// </summary>
    string PageImageFileName(string documentName, uint pageIndex, uint totalPages, PdfCoreImageFormat format);

    /// <summary>
    /// The first of <paramref name="pages"/> too large to raster at
    /// <paramref name="dpi"/>, or <c>null</c> when every one fits — asked
    /// before an export writes its first file.
    /// </summary>
    uint? FirstPageTooLargeToExport(IPdfCoreDocument document, IReadOnlyList<uint> pages, uint dpi);

    /// <summary>One page rendered and encoded — the bytes of the file to write.</summary>
    byte[] ExportPageImage(IPdfCoreDocument document, uint pageIndex, uint dpi, PdfCoreImageFormat format);

    /// <summary>
    /// Whether <paramref name="document"/> could survive a full rewrite — the
    /// second gate <see cref="ExtractPagesToPdf"/> applies, beyond
    /// <see cref="TextExtractionAllowed"/>: a new page set can only come from a
    /// full rewrite, and an encrypted document opened with only one of its two
    /// passwords can never be re-encrypted. Asked up front so a shell can
    /// refuse before offering the Extract dialog at all.
    /// </summary>
    bool FullRewriteAllowed(IPdfCoreDocument document);

    /// <summary>
    /// Prunes a clone of <paramref name="document"/> down to
    /// <paramref name="pages"/> (zero-based, ascending, deduplicated
    /// positions) and returns the bytes of the resulting PDF. The live
    /// document is not mutated.
    /// </summary>
    byte[] ExtractPagesToPdf(IPdfCoreDocument document, IReadOnlyList<uint> pages);

    /// <summary>
    /// Whether the document <paramref name="document"/> was opened from
    /// carries a digital signature — the one fact an extraction summary needs
    /// beyond the page count.
    /// </summary>
    bool ExtractSourceIsSigned(IPdfCoreDocument document);

    /// <summary>Shared split boundaries and safe file names from the Rust core.</summary>
    IReadOnlyList<SplitPart> PlanSplit(string cuts, uint totalPages, string documentName);
}

/// <summary>Mirrors <c>FfiExportFormat</c>.</summary>
internal enum PdfCoreImageFormat { Png, Jpeg }

/// <summary>Mirrors <c>FfiCompressPreset</c>; the numbers behind each stay in the core.</summary>
internal enum PdfCoreCompressPreset { Lossless, Balanced, Small }

/// <summary>
/// A compressed save: the bytes and what was done to them, as one value so
/// the bytes never travel without the report. <see cref="Refusals"/> are the
/// core's own sentences.
/// </summary>
internal sealed record PdfCoreCompressedSave(
    byte[] Bytes,
    ulong BeforeBytes,
    ulong AfterBytes,
    ulong SavedBytes,
    bool Reduced,
    IReadOnlyList<string> Refusals);

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

/// <summary>
/// How a text run's font can be re-encoded — the one property that decides
/// whether a run can preserve its font when retyped. A composite (Type0/CID)
/// run uses the core's explicit inserted-font substitution command instead.
/// </summary>
internal enum PdfCoreFontKind { Standard14, EmbeddedSimple, EmbeddedComposite }

internal sealed record PdfCoreContentTextRun(ulong Id, uint PageIndex, PdfCoreRect Bbox, string ResourceFontName, PdfCoreFontKind FontKind, string Text);
/// <summary>
/// An image painted by a page's content stream. <c>ResourceXObjectName</c> is
/// null when the image is <em>inline</em> — its samples live in the content
/// stream itself, so there is no <c>/Resources /XObject</c> key naming it.
/// The core models that as an enum; the boundary carries the same information
/// as an optional name (see <c>FfiContentImageItem</c>).
/// </summary>
internal sealed record PdfCoreContentImage(ulong Id, uint PageIndex, PdfCoreRect Bbox, string? ResourceXObjectName);
internal sealed record PdfCorePageContent(IReadOnlyList<PdfCoreContentTextRun> TextRuns, IReadOnlyList<PdfCoreContentImage> Images);
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

    public sealed record SetDocumentInfo(PdfCoreDocumentInfo After) : PdfCoreEdit;

    // Page structure. Every page number is a position in the document's
    // current order — the core resolves it to the page sitting there.
    public sealed record RotatePage(uint PageIndex, int DeltaDegrees) : PdfCoreEdit;
    public sealed record RemovePage(uint PageIndex) : PdfCoreEdit;
    /// <summary><paramref name="To"/> is where the block starts after the move.</summary>
    public sealed record MovePages(uint From, uint Count, uint To) : PdfCoreEdit;

    /// <summary>
    /// Fills an existing field in. The core validates <paramref name="Value"/>
    /// against the field's kind before recording it.
    /// </summary>
    public sealed record SetFieldValue(ulong FieldId, FormFieldValue Value) : PdfCoreEdit;

    /// <summary>
    /// Retypes an existing text run, keeping its font, size and position.
    /// Carries the run as it was read from <see cref="IPdfCore.ReadPageContent"/>:
    /// that snapshot is how the core re-finds the run at save time, so it is
    /// passed back unchanged rather than rebuilt from what is on screen.
    /// </summary>
    public sealed record ReplaceTextRun(PdfCoreContentTextRun Item, string After) : PdfCoreEdit;
    public sealed record ReplaceTextRunWithInsertedFont(PdfCoreContentTextRun Item, string After) : PdfCoreEdit;
}

internal sealed record PdfCoreFormField(ulong Id, uint PageIndex, string Name, FormFieldKind Kind, FormFieldValue Value);

internal sealed record PdfCoreDocumentInfo(
    string? Title,
    string? Author,
    string? Subject,
    string? Keywords,
    string? Creator,
    string? Producer,
    object? CreationDate,
    object? ModDate);

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
    FormFieldNotFound,
    InvalidImage,
    InvalidSaveRequest,
    SignaturesWouldBeInvalidated,
    UnsupportedOperation,
    /// <summary>
    /// The replacement text needs a character the run's font cannot
    /// represent. Its own category because it is the one content failure the
    /// reader can fix by typing something else, and the message names the
    /// character.
    /// </summary>
    EncodingGap,
    /// <summary>
    /// A typed page range the core could not read. Like
    /// <see cref="EncodingGap"/>, the reader can fix it, and the core's
    /// sentence saying how travels in <see cref="PdfCoreException.ReaderFacingDetail"/>.
    /// </summary>
    InvalidPageSelection,
    RenderFailed,
    Io,
    UnsavedChanges,
    Internal
}

internal sealed class PdfCoreException : Exception
{
    public PdfCoreException(PdfCoreError category, string diagnosticDetail, string? readerFacingDetail = null) : base(diagnosticDetail)
    {
        Category = category;
        ReaderFacingDetail = readerFacingDetail;
    }

    public PdfCoreError Category { get; }

    /// <summary>
    /// The one fragment of this failure that is safe — and useful — to put in
    /// front of the reader, or <c>null</c> when the category alone says
    /// everything. <see cref="PdfCoreError.EncodingGap"/> fills it in with the
    /// character that could not be encoded: the reader typed it, and naming it
    /// is the difference between "not supported" and "not that character".
    /// <see cref="PdfCoreError.InvalidPageSelection"/> fills it in with the
    /// core's sentence about the range the reader typed.
    /// </summary>
    public string? ReaderFacingDetail { get; }
}

internal interface IDiagnosticLogger
{
    void Failure(PdfCoreError category, string operation, string correlationId, string? sessionId, uint? pageIndex, string sanitizedDetail);
}
