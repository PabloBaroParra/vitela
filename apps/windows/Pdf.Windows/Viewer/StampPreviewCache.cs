using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>
/// Presentation-only stamp previews scoped to one document session. The PDF
/// remains the source of truth. Entries are keyed by annotation id and are
/// only ever dropped when the session changes, which is what lets redo repaint
/// the original image without another input path: undo and redo carry the
/// annotation value itself (see <c>Command::inverse</c> in <c>edit_log.rs</c>),
/// so a redone stamp comes back under the id its preview was filed under.
/// </summary>
public sealed class StampPreviewCache<T> where T : class
{
    private readonly Dictionary<ulong, T> _previews = [];
    private string? _sessionId;

    /// <summary>
    /// Points the cache at <paramref name="sessionId"/>, clearing previews that
    /// belonged to the document being replaced, and reports whether that
    /// happened. Re-declaring the session already in effect is a no-op that
    /// keeps the existing previews — the facade mints a fresh GUID per open, so
    /// today no caller can hit that branch; it is here so a future caller that
    /// re-announces a live session cannot silently blank the stamps.
    /// </summary>
    public bool BeginSession(string sessionId)
    {
        ArgumentException.ThrowIfNullOrEmpty(sessionId);
        if (string.Equals(_sessionId, sessionId, StringComparison.Ordinal)) return false;
        _sessionId = sessionId;
        _previews.Clear();
        return true;
    }

    public bool IsCurrent(string sessionId) => ImageStampInput.SessionMatches(sessionId, _sessionId);

    public void Set(string sessionId, ulong annotationId, T preview)
    {
        if (IsCurrent(sessionId)) _previews[annotationId] = preview;
    }

    public bool TryGet(ulong annotationId, out T? preview) => _previews.TryGetValue(annotationId, out preview);
}

/// <summary>Reconciles a successful insertion snapshot with its new stamp ID.</summary>
public static class StampPreviewReconciliation
{
    public static ulong? InsertedStampId(IReadOnlyList<Annotation> before, IReadOnlyList<Annotation> after)
    {
        var previousIds = before.Select(annotation => annotation.Id).ToHashSet();
        var inserted = after
            .Where(annotation => annotation.Kind == AnnotationKind.Stamp && !previousIds.Contains(annotation.Id))
            .Select(annotation => annotation.Id)
            .ToList();
        return inserted.Count == 1 ? inserted[0] : null;
    }
}
