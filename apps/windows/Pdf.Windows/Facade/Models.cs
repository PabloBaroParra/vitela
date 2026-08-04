namespace Pdf.Windows.Facade;

public sealed record DocumentSource(string DisplayName, byte[] Bytes);

public sealed record DocumentSession(string SessionId, string DisplayName, uint PageCount, uint PageIndex, DocumentSessionState State, IReadOnlyList<PageDimensions> Pages);

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
