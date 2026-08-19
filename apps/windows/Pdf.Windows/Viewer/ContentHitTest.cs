using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>
/// Which run of page text a click landed on. Kept out of the WinUI partial so
/// the rule can be exercised without a UI runtime, the same reason
/// <see cref="PageZoom"/> and <see cref="ViewportTilePlan"/> live here.
/// </summary>
public static class ContentHitTest
{
    /// <summary>
    /// The text run whose box contains (<paramref name="xPt"/>,
    /// <paramref name="yPt"/>) in PDF space, or <c>null</c> when the point
    /// lands outside every run on the page.
    /// </summary>
    /// <remarks>
    /// Overlaps are broken by smallest box, matching the GTK shell's
    /// <c>content_edit::model::text_run_at</c>: this mode is about picking out
    /// one run to retype, and the smaller of two overlapping runs is the more
    /// specific — and so the more likely intended — target.
    /// </remarks>
    public static ContentTextRun? TextRunAt(IReadOnlyList<ContentTextRun> runs, double xPt, double yPt)
    {
        ContentTextRun? best = null;
        var bestArea = double.MaxValue;
        foreach (var run in runs)
        {
            var bounds = run.Bounds;
            if (xPt < bounds.X || xPt > bounds.X + bounds.Width || yPt < bounds.Y || yPt > bounds.Y + bounds.Height)
            {
                continue;
            }

            var area = bounds.Width * bounds.Height;
            if (area < bestArea)
            {
                best = run;
                bestArea = area;
            }
        }

        return best;
    }
}
