namespace Pdf.Windows.Viewer;

/// <summary>What the status line says once a watermark is recorded.</summary>
/// <remarks>
/// Mirrors the Linux shell's <c>content_edit::watermark::options::summary</c>,
/// plus the "Save to keep the change." every other Windows content edit says:
/// the stamp is queued, not yet written.
/// </remarks>
public static class WatermarkWording
{
    public static string Summary(int count)
    {
        var pages = count == 1 ? "page" : "pages";
        return $"Watermark added to {count} {pages}. Save to keep the change.";
    }
}
