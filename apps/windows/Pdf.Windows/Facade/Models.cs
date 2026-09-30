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

/// <summary>
/// One page's layout size in PDF points (1/72 inch), as it is drawn: a
/// quarter <paramref name="Rotation"/> has already swapped the width and the
/// height.
/// </summary>
/// <param name="Rotation">
/// The turn that size includes. Everything <em>on</em> the page — text runs,
/// search hits, annotations — stays in the page's unrotated space, so an
/// overlay needs the turn as well as the size to land where the page drew it.
/// </param>
public sealed record PageDimensions(double WidthPt, double HeightPt, PageRotation Rotation);

/// <summary>The clockwise quarter-turn a page is drawn with.</summary>
public enum PageRotation
{
    None,
    Clockwise90,
    Clockwise180,
    Clockwise270,
}

/// <summary>
/// One page as it is drawn on screen: its size with the turn applied (what
/// <see cref="PageDimensions"/> reports), the turn, and the display units per
/// point it is shown at. Everything the PDF-to-screen transform needs.
/// </summary>
public sealed record PagePlacement(double WidthPt, double HeightPt, PageRotation Rotation, double Scale)
{
    public static PagePlacement Of(PageDimensions page, double scale) => new(page.WidthPt, page.HeightPt, page.Rotation, scale);

    /// <summary>The turn in degrees clockwise — what a <c>RotateTransform</c> takes.</summary>
    public double Degrees => Rotation switch
    {
        PageRotation.Clockwise90 => 90,
        PageRotation.Clockwise180 => 180,
        PageRotation.Clockwise270 => 270,
        _ => 0,
    };
}

/// <summary>A rect on the drawn page: top-left origin, y growing downwards, in display units.</summary>
public sealed record PlacedRect(double Left, double Top, double Width, double Height);

/// <summary>A point on the drawn page: top-left origin, y growing downwards, in display units.</summary>
public sealed record PlacedPoint(double Left, double Top);

/// <summary>
/// A change to the document's pages. Every page number is the page's current
/// position — the one the viewer shows it at — never a stable identity.
/// </summary>
public abstract record PageEdit
{
    /// <summary>Inserts an A4 page at a position, including after the last page.</summary>
    public sealed record InsertBlank(uint Index, PageOrientation Orientation = PageOrientation.Portrait) : PageEdit;
    /// <summary>Turns a page clockwise by <paramref name="DeltaDegrees"/>; negative turns it back.</summary>
    public sealed record Rotate(uint PageIndex, int DeltaDegrees) : PageEdit;
    public sealed record Remove(uint PageIndex) : PageEdit;
    /// <summary>Moves one page so that it ends up at position <paramref name="To"/>.</summary>
    public sealed record Move(uint From, uint To) : PageEdit;
}

public enum PageOrientation
{
    Portrait,
    Landscape,
}

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

/// <summary>How a text run's font can be re-encoded.</summary>
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
/// the facade's text-replacement methods unchanged rather than being rebuilt
/// from what the reader sees.
/// </remarks>
public sealed class ContentTextRun
{
    private readonly PdfCoreContentTextRun _source;

    internal ContentTextRun(PdfCoreContentTextRun source, string? baseFont, string? sessionId = null, ulong revision = 0)
    {
        _source = source;
        SessionId = sessionId;
        Revision = revision;
        Bounds = new AnnotationRect(source.Bbox.X, source.Bbox.Y, source.Bbox.Width, source.Bbox.Height);
        FontKind = (ContentFontKind)source.FontKind;
        BaseFont = baseFont;
    }

    internal PdfCoreContentTextRun Source => _source;
    internal string? SessionId { get; }
    internal ulong Revision { get; }

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
    /// Whether retyping this run requires replacing its composite font with
    /// the standard font used for inserted text.
    /// </summary>
    public bool RequiresFontSubstitution => FontKind == ContentFontKind.EmbeddedComposite;

    /// <summary>Every known font kind is editable, directly or through explicit substitution.</summary>
    public bool IsEditable => true;
}

/// <summary>
/// One page's editable content, parsed on demand.
/// </summary>
/// <remarks>
/// Text runs for the inline editor. Image tools read a separate, revision-bound
/// snapshot through <see cref="PdfDocumentFacade.PageImagesAsync"/>.
///
/// The runs are valid against the bytes the document was opened from. They
/// survive a preview refresh, which leaves those bytes alone, but a save and
/// reopen invalidates every id — re-read the page after one.
/// </remarks>
public sealed record PageContent(uint PageIndex, IReadOnlyList<ContentTextRun> TextRuns);

/// <summary>A page bound to the document revision at which the insertion dialog opened.</summary>
public sealed class TextInsertionTarget
{
    internal TextInsertionTarget(string sessionId, ulong revision, uint pageIndex)
        => (SessionId, Revision, PageIndex) = (sessionId, revision, pageIndex);
    internal string SessionId { get; }
    internal ulong Revision { get; }
    public uint PageIndex { get; }
}

/// <summary>An image snapshot bound to the session and revision that parsed it.</summary>
public sealed class ContentImage
{
    internal ContentImage(PdfCoreContentImage source, string sessionId, ulong revision)
    {
        Source = source;
        SessionId = sessionId;
        Revision = revision;
    }

    internal PdfCoreContentImage Source { get; }
    internal string SessionId { get; }
    internal ulong Revision { get; }
    public ulong Id => Source.Id;
    public uint PageIndex => Source.PageIndex;
    public AnnotationRect Bounds => new(Source.Bbox.X, Source.Bbox.Y, Source.Bbox.Width, Source.Bbox.Height);
}

public enum AnnotationKind { Highlight, Underline, Strikeout, Ink, Shape, TextNote, Stamp }
public sealed record AnnotationRect(double X, double Y, double Width, double Height);
public sealed record AnnotationColor(byte R, byte G, byte B);
public sealed record AnnotationPoint(double X, double Y);
public sealed record Annotation(ulong Id, uint PageIndex, AnnotationKind Kind, AnnotationRect? Rect, AnnotationColor? Color, IReadOnlyList<AnnotationPoint> Points);
public sealed record AnnotationState(string SessionId, IReadOnlyList<Annotation> Annotations, bool EditingAllowed, bool CanUndo, bool CanRedo);
public sealed record DocumentInfo(string? Title, string? Author, string? Subject, string? Keywords, string? Creator, string? Producer);

/// <summary>
/// What a form field can hold. Mirrors the core's four fillable kinds;
/// pushbuttons, listboxes and signature fields never reach a shell.
/// </summary>
public abstract record FormFieldKind
{
    /// <summary><paramref name="MaxLength"/> is the field's <c>/MaxLen</c>, when it has one.</summary>
    public sealed record Text(bool Multiline, uint? MaxLength) : FormFieldKind;
    public sealed record Checkbox : FormFieldKind;
    /// <summary>The export value of each button, in the order the file lists them.</summary>
    public sealed record RadioGroup(IReadOnlyList<string> Options) : FormFieldKind;
    /// <summary>An editable dropdown also accepts text that is not one of its options.</summary>
    public sealed record Dropdown(IReadOnlyList<string> Options, bool Editable) : FormFieldKind;
}

/// <summary>
/// What a field holds right now. Which variant fits is the field's
/// <see cref="FormFieldKind"/>'s call, and the core checks it before recording.
/// </summary>
public abstract record FormFieldValue
{
    public sealed record Text(string Value) : FormFieldValue;
    public sealed record Checked(bool Value) : FormFieldValue;
    /// <summary><c>null</c> is nothing chosen — no radio on, no dropdown value.</summary>
    public sealed record Choice(string? Option) : FormFieldValue;
}

public enum FormFont { Helvetica, TimesRoman, Courier }
public sealed record FormTextStyle(FormFont Font, double SizePt, AnnotationColor Color);
public sealed record FormField(ulong Id, uint PageIndex, string Name, FormFieldKind Kind, FormFieldValue Value, FormTextStyle? Style = null, AnnotationRect? Rect = null);

/// <summary>
/// The fill panel's snapshot. <paramref name="FillAllowed"/> is the document's
/// own answer to "may a reader fill this form in" — narrower than content
/// editing, and never derived from it. <paramref name="StructureAllowed"/> is
/// the core's form-field editing answer — may fields be placed, moved,
/// renamed or restyled — which is not content editing either.
/// </summary>
public sealed record FormFieldState(string SessionId, IReadOnlyList<FormField> Fields, bool FillAllowed, bool StructureAllowed);
public sealed record SavedDocument(byte[] Bytes, ulong EditRevision);

/// <summary>How hard a compression tries. Three, not a slider — see <c>pdf_ffi::compress</c>.</summary>
public enum CompressionPreset { Lossless, Balanced, Small }

/// <summary>
/// What a compression produced. <see cref="BeforeBytes"/> is the size of an
/// uncompressed save of the session, not of the file on disk.
/// <see cref="Reduced"/> is <c>false</c> when the core handed the save's own
/// bytes back because nothing smaller came out — a success, not a failure.
/// </summary>
public sealed record CompressionResult(byte[] Bytes, ulong BeforeBytes, ulong AfterBytes, ulong SavedBytes, bool Reduced, IReadOnlyList<string> Refusals);

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
