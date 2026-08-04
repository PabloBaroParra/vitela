namespace Pdf.Windows.Viewer;

/// <summary>Guards stamp work against the document changing underneath it.</summary>
public static class ImageStampInput
{
    /// <summary>
    /// Whether the session a stamp operation started against is still the one
    /// on screen. Inserting a stamp decodes a preview and crosses several
    /// awaits, any of which the reader can spend opening a different document,
    /// and neither the status line nor the preview cache belongs to that new
    /// document.
    /// </summary>
    public static bool SessionMatches(string capturedSessionId, string? currentSessionId) =>
        string.Equals(capturedSessionId, currentSessionId, StringComparison.Ordinal);
}
