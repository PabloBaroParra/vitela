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

public sealed record UserSafeError(string Message, string CorrelationId);

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
