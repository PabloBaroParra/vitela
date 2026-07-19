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

/// <summary>PDF-space geometry with a bottom-left origin, in points.</summary>
public sealed record SearchRect(double XPt, double YPt, double WidthPt, double HeightPt);

public sealed record SearchHit(uint PageIndex, string Text, IReadOnlyList<SearchRect> CharacterBounds);

public sealed record SearchResults(string SessionId, ulong Sequence, string Query, IReadOnlyList<SearchHit> Hits);

/// <summary>
/// A failure the UI can show verbatim: <see cref="Message"/> never leaks
/// diagnostics, and <see cref="CorrelationId"/> ties it back to the log.
/// <see cref="RequiresPassword"/> is the one actionable bit the UI needs to
/// tell an encrypted document (prompt for a password and retry) apart from a
/// dead-end failure — it carries no sensitive detail.
/// </summary>
public sealed record UserSafeError(string Message, string CorrelationId, bool RequiresPassword = false);

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

public sealed record SearchResult(SearchResults? Value, UserSafeError? Error, bool IsDiscarded)
{
    public bool IsSuccess => Error is null && !IsDiscarded;

    public static SearchResult Success(SearchResults value) => new(value, null, false);
    public static SearchResult Discarded() => new(null, null, true);
    public static SearchResult Failure(UserSafeError error) => new(null, error, false);
}
