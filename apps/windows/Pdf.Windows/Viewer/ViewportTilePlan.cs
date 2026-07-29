namespace Pdf.Windows.Viewer;

/// <summary>A page-local rectangle in DIPs, measured from its top-left corner.</summary>
public readonly record struct ViewportRect(double LeftDips, double TopDips, double WidthDips, double HeightDips);

/// <summary>An integer tile in the renderer's top-left pixel coordinate space.</summary>
public readonly record struct TileRequest(int LeftPx, int TopPx, int WidthPx, int HeightPx, bool IsVisible);

/// <summary>
/// Chooses bounded, integer-aligned tiles only when a full-page bitmap would
/// lower the requested DPI. Integer edges are shared by neighboring tiles, so
/// composition cannot introduce rounding gaps.
///
/// Tiles are anchored to a fixed grid in the page's own pixel space, never to
/// the scroll offset. A grid that moved with the viewport would produce a
/// different tile set for every scrolled pixel, so nothing already rendered
/// could ever be reused — each tick would re-rasterize the whole viewport and
/// orphan the render still in flight.
/// </summary>
public sealed class ViewportTilePlan
{
    public const long MaxTilePixels = 1_048_576;
    public const int TileEdgePixels = 1024;

    private ViewportTilePlan(bool usesTiles, uint dpi, IReadOnlyList<TileRequest> visibleTiles)
    {
        UsesTiles = usesTiles;
        Dpi = dpi;
        VisibleTiles = visibleTiles;
    }

    public bool UsesTiles { get; }
    public uint Dpi { get; }
    public IReadOnlyList<TileRequest> VisibleTiles { get; }
    public bool CoversViewportWithoutGaps => VisibleTiles.Count > 0 && VisibleTiles.All(tile => tile.WidthPx > 0 && tile.HeightPx > 0);

    /// <summary>
    /// The DPI the display actually wants for this page, before the full-page
    /// raster budget is applied.
    /// </summary>
    public static uint IntendedDpi(PageBox box, double rasterizationScale)
    {
        var displayScale = double.IsFinite(rasterizationScale) && rasterizationScale > 0 ? rasterizationScale : 1.0;
        return (uint)Math.Min(PageZoom.MaxRenderDpi, Math.Floor(box.Factor * 96 * displayScale));
    }

    /// <summary>
    /// Whether this page will be tiled, without needing a viewport. The layout
    /// pass asks this to decide whether the page's full-page bitmap is a
    /// bridge (see <see cref="PageZoom.BridgeDpi"/>) or the real thing, and it
    /// must give the same answer <see cref="Resolve"/> would.
    /// </summary>
    public static bool WouldUseTiles(PageBox box, double rasterizationScale) =>
        IntendedDpi(box, rasterizationScale) > box.RenderDpi;

    public static ViewportTilePlan Resolve(double pageWidthPt, double pageHeightPt, PageBox box, ViewportRect viewport, double rasterizationScale)
    {
        var intendedDpi = IntendedDpi(box, rasterizationScale);
        if (intendedDpi <= box.RenderDpi)
        {
            return new ViewportTilePlan(false, box.RenderDpi, []);
        }

        var pixelsPerDip = intendedDpi / (box.Factor * 96.0);
        var pageWidthPx = (int)Math.Ceiling(pageWidthPt * intendedDpi / 72.0);
        var pageHeightPx = (int)Math.Ceiling(pageHeightPt * intendedDpi / 72.0);
        var left = Math.Clamp((int)Math.Floor(viewport.LeftDips * pixelsPerDip), 0, pageWidthPx);
        var top = Math.Clamp((int)Math.Floor(viewport.TopDips * pixelsPerDip), 0, pageHeightPx);
        var right = Math.Clamp((int)Math.Ceiling((viewport.LeftDips + viewport.WidthDips) * pixelsPerDip), left, pageWidthPx);
        var bottom = Math.Clamp((int)Math.Ceiling((viewport.TopDips + viewport.HeightDips) * pixelsPerDip), top, pageHeightPx);
        // Snap the covered rectangle out to whole grid cells. Cells are clipped
        // by the page, not by the viewport, so the same on-screen pixel always
        // belongs to the same tile no matter where the reader has scrolled.
        var tiles = new List<TileRequest>();
        for (var y = SnapToGrid(top); y < bottom; y += TileEdgePixels)
        {
            for (var x = SnapToGrid(left); x < right; x += TileEdgePixels)
            {
                tiles.Add(new TileRequest(x, y, Math.Min(TileEdgePixels, pageWidthPx - x), Math.Min(TileEdgePixels, pageHeightPx - y), true));
            }
        }

        return new ViewportTilePlan(true, intendedDpi, tiles);
    }

    private static int SnapToGrid(int pixel) => pixel / TileEdgePixels * TileEdgePixels;

    public static ViewportTilePlan ForTesting(IReadOnlyList<TileRequest> tiles) => new(true, PageZoom.MaxRenderDpi, tiles);

    public TileRequest? NextRequest() => VisibleTiles.FirstOrDefault(tile => tile.IsVisible);
}
