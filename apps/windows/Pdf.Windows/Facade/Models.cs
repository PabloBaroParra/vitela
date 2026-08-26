namespace Pdf.Windows.Facade;

public sealed record DocumentSource(string DisplayName, byte[] Bytes);

/// <summary>
/// One open document, as the shell sees it.
/// </summary>
/// <param name="ContentEditingAllowed">
/// Whether this document permits rewriting the text and images on its pages.
/// A session-lifetime answer — the document's own permissions decide it and
/// they cannot change while it is open — which is why it rides here rather
/// than being re-asked per edit. Gated by a different <c>/P</c> bit than
/// annotation editing (<see cref="AnnotationState.EditingAllowed"/>): a
/// document can allow one and refuse the other.
/// </param>
public sealed record DocumentSession(string SessionId, string DisplayName, uint PageCount, uint PageIndex, DocumentSessionState State, IReadOnlyList<PageDimensions> Pages, bool ContentEditingAllowed);

/// <summary>One page's layout size in PDF points (1/72 inch).</summary>
public sealed record PageDimensions(double WidthPt, double HeightPt);

public enum DocumentSessionState
{
    Empty,
    Ready
}

public sealed record RenderedPage(string SessionId, uint PageIndex, ulong Sequence, uint Width, uint Height, uint Stride, byte[] Rgba);

/// <summary>A top-left page pixel rectangle at the render DPI.</summary>
public sealed record PageRegion(uint LeftPx, uint TopPx, uint WidthPx, uint HeightPx);

/// <summary>PDF-space geometry with a bottom-left origin, in points.</summary>
public sealed record SearchRect(double XPt, double YPt, double WidthPt, double HeightPt);

public sealed record SearchHit(uint PageIndex, string Text, IReadOnlyList<SearchRect> CharacterBounds);

public sealed record SearchResults(string SessionId, ulong Sequence, string Query, IReadOnlyList<SearchHit> Hits);

/// <summary>
/// One page's characters, ready for repeated caret hit-testing and
/// selection-rect queries without another round trip to the core. Facade
/// callers hold this for the life of a drag-select (or the page's lifetime)
/// and must dispose it when done, same as <see cref="SavedDocument"/>'s
/// sibling native-backed types.
/// </summary>
public sealed class PageCharacters : IDisposable
{
    private readonly IPdfCorePageCharacters _handle;

    internal PageCharacters(uint pageIndex, IPdfCorePageCharacters handle)
    {
        PageIndex = pageIndex;
        _handle = handle;
    }

    public uint PageIndex { get; }

    /// <summary>The caret nearest a PDF-space point, or <c>null</c> on a page with no positioned text.</summary>
    public uint? CaretAt(double xPt, double yPt) => _handle.CaretAt(xPt, yPt);

    /// <summary>The text between two carets, for the clipboard. Order does not matter.</summary>
    public string TextIn(uint anchor, uint focus) => _handle.TextIn(anchor, focus);

    /// <summary>The rects a shell paints between two carets: one per visual line.</summary>
    public IReadOnlyList<SearchRect> RectsIn(uint anchor, uint focus) =>
        [.. _handle.RectsIn(anchor, focus).Select(rect => new SearchRect(rect.XPt, rect.YPt, rect.WidthPt, rect.HeightPt))];

    public void Dispose() => _handle.Dispose();
}

/// <summary>How a text run's font can be re-encoded — see <see cref="ContentTextRun.IsEditable"/>.</summary>
public enum ContentFontKind { Standard14, EmbeddedSimple, EmbeddedComposite }

/// <summary>
/// One run of text painted by a page's own content stream — the thing
/// content-edit mode retypes, as opposed to an annotation drawn over the
/// page.
/// </summary>
/// <remarks>
/// A class rather than a record because it carries the core's snapshot of
/// the run privately: that snapshot is how the core re-finds this run when
/// the edit is written, so it travels back into
/// <see cref="PdfDocumentFacade.ReplaceTextRunAsync"/> unchanged rather than
/// being rebuilt from what the reader sees.
/// </remarks>
public sealed class ContentTextRun
{
    private readonly PdfCoreContentTextRun _source;

    internal ContentTextRun(PdfCoreContentTextRun source, string? baseFont)
    {
        _source = source;
        Bounds = new AnnotationRect(source.Bbox.X, source.Bbox.Y, source.Bbox.Width, source.Bbox.Height);
        FontKind = (ContentFontKind)source.FontKind;
        BaseFont = baseFont;
    }

    internal PdfCoreContentTextRun Source => _source;

    public ulong Id => _source.Id;
    public uint PageIndex => _source.PageIndex;

    /// <summary>The run's box in PDF space, bottom-left origin — where an inline editor goes.</summary>
    public AnnotationRect Bounds { get; }

    public ContentFontKind FontKind { get; }

    /// <summary>
    /// The font this run is painted with, as the file names it —
    /// <c>Helvetica-Bold</c>, <c>ABCDEF+Times New Roman</c> — or <c>null</c>
    /// when the page's resources do not say.
    /// </summary>
    /// <remarks>
    /// Raw on purpose. Which local font stands in for it is a platform
    /// decision, and one the core has no business making.
    /// </remarks>
    public string? BaseFont { get; }

    public string Text => _source.Text;

    /// <summary>
    /// Whether this run can be retyped at all. A composite (Type0/CID) font
    /// cannot: extending a subsetted CID font's glyph coverage means
    /// re-subsetting it, which this version does not do, so the core refuses
    /// every replacement against one. The shell asks here before opening an
    /// editor, rather than letting the reader type into a box that can only
    /// fail.
    /// </summary>
    public bool IsEditable => FontKind != ContentFontKind.EmbeddedComposite;
}

/// <summary>
/// One page's editable content, parsed on demand.
/// </summary>
/// <remarks>
/// Text runs only for now. The core also reports the page's images, and the
/// image half of content editing (select/move/resize/replace) is the next
/// slice; exposing an <c>Images</c> list before anything can act on it would
/// be a contract nothing honours.
///
/// The runs are valid against the bytes the document was opened from. They
/// survive a preview refresh, which leaves those bytes alone, but a save and
/// reopen invalidates every id — re-read the page after one.
/// </remarks>
public sealed record PageContent(uint PageIndex, IReadOnlyList<ContentTextRun> TextRuns);

public enum AnnotationKind { Highlight, Underline, Strikeout, Ink, Shape, TextNote, Stamp }
public sealed record AnnotationRect(double X, double Y, double Width, double Height);
public sealed record AnnotationColor(byte R, byte G, byte B);
public sealed record AnnotationPoint(double X, double Y);
public sealed record Annotation(ulong Id, uint PageIndex, AnnotationKind Kind, AnnotationRect? Rect, AnnotationColor? Color, IReadOnlyList<AnnotationPoint> Points);
public sealed record AnnotationState(string SessionId, IReadOnlyList<Annotation> Annotations, bool EditingAllowed, bool CanUndo, bool CanRedo);
public sealed record SavedDocument(byte[] Bytes, ulong EditRevision);

/// <summary>
/// A failure the UI can show verbatim: <see cref="Message"/> never leaks
/// diagnostics, and <see cref="CorrelationId"/> ties it back to the log.
///
/// Two flags mark the failures a reader can act on rather than only read.
/// <see cref="RequiresPassword"/> means an encrypted document: prompt and
/// retry. <see cref="RequiresPendingEditDecision"/> means the open was refused
/// to protect unsaved annotation work: ask what to do with it and retry.
/// Neither carries sensitive detail; everything else is a dead end.
/// </summary>
public sealed record UserSafeError(string Message, string CorrelationId, bool RequiresPassword = false, bool RequiresPendingEditDecision = false);

public sealed record OperationResult<T>(T? Value, UserSafeError? Error)
{
    public bool IsSuccess => Error is null;

    public static OperationResult<T> Success(T value) => new(value, null);

    public static OperationResult<T> Failure(UserSafeError error) => new(default, error);
}

public sealed record RenderResult(RenderedPage? Value, UserSafeError? Error, bool IsEmpty, bool IsDiscarded)
{
    public bool IsSuccess => Error is null && !IsEmpty && !IsDiscarded;

    public static RenderResult Success(RenderedPage value) => new(value, null, false, false);

    public static RenderResult Empty() => new(null, null, true, false);

    public static RenderResult Discarded() => new(null, null, false, true);

    public static RenderResult Failure(UserSafeError error) => new(null, error, false, false);
}

/// <summary>
/// The result of one viewport-covering tile batch. A batch succeeds or fails
/// whole: a half-covered viewport is worse than none, because the reader would
/// see sharp text next to the stretched base bitmap with no way to tell the
/// difference from a rendering bug.
/// </summary>
public sealed record TileBatchResult(IReadOnlyList<RenderedPage>? Value, UserSafeError? Error, bool IsDiscarded)
{
    public bool IsSuccess => Error is null && !IsDiscarded;

    public static TileBatchResult Success(IReadOnlyList<RenderedPage> value) => new(value, null, false);

    public static TileBatchResult Discarded() => new(null, null, true);

    public static TileBatchResult Failure(UserSafeError error) => new(null, error, false);
}

public sealed record SearchResult(SearchResults? Value, UserSafeError? Error, bool IsDiscarded)
{
    public bool IsSuccess => Error is null && !IsDiscarded;

    public static SearchResult Success(SearchResults value) => new(value, null, false);
    public static SearchResult Discarded() => new(null, null, true);
    public static SearchResult Failure(UserSafeError error) => new(null, error, false);
}
