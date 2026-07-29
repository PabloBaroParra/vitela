namespace Pdf.Windows.Viewer;

/// <summary>What the user picked, not what it currently resolves to.</summary>
public enum PageZoomMode
{
    /// <summary>Each page is scaled so its width fills the viewport.</summary>
    FitWidth,
    /// <summary>Each page is scaled so the whole page fits in the viewport.</summary>
    FitPage,
    /// <summary>An explicit factor the user chose; 1.0 is 100%.</summary>
    Custom
}

/// <summary>
/// The viewer's zoom setting. <see cref="Factor"/> is only meaningful for
/// <see cref="PageZoomMode.Custom"/>; the fit modes derive theirs per page.
/// </summary>
public readonly record struct ZoomSetting(PageZoomMode Mode, double Factor)
{
    public static readonly ZoomSetting FitWidth = new(PageZoomMode.FitWidth, 1.0);
    public static readonly ZoomSetting FitPage = new(PageZoomMode.FitPage, 1.0);

    public static ZoomSetting Custom(double factor) => new(PageZoomMode.Custom, PageZoom.ClampFactor(factor));
}

/// <summary>The scrollable area available to pages, in DIPs.</summary>
public readonly record struct ViewportSize(double WidthDips, double HeightDips);

/// <summary>
/// One page's resolved geometry: the on-screen box, the DPI its bitmap should
/// be rendered at, and the zoom factor that produced them (what the UI shows
/// as "150%").
/// </summary>
public readonly record struct PageBox(double WidthDips, double HeightDips, uint RenderDpi, double Factor);

/// <summary>
/// Zoom and page-box geometry, deliberately free of any WinUI type so it can
/// be verified without a UI runtime. The view owns the chrome; this owns the
/// arithmetic.
/// </summary>
public static class PageZoom
{
    public const double MinFactor = 0.10;
    public const double MaxFactor = 8.0;

    /// <summary>
    /// A PDF point is 1/72 inch and a DIP is 1/96 inch, so at 100% one point
    /// covers 4/3 of a DIP. Everything else scales off this.
    /// </summary>
    public const double DipsPerPointAt100 = 96.0 / 72.0;

    public const uint MinRenderDpi = 24;
    public const uint MaxRenderDpi = 600;

    /// <summary>
    /// Ceiling on one page's bitmap, in pixels. At deep zoom the box keeps
    /// growing but the bitmap does not: past this point the renderer output is
    /// upscaled to fill the box instead of allocating without bound (RGBA8, so
    /// this ceiling is four bytes per pixel of peak page memory).
    /// </summary>
    public const long MaxRenderPixels = 8_000_000;

    /// <summary>
    /// Ceiling on the full-page bitmap of a page that is also being tiled.
    ///
    /// Once tiles carry the detail, the full-page bitmap is only ever seen
    /// stretched — while the tiles are still rendering, and wherever a tile has
    /// not landed. It has to be produced fast and cost little on the shared
    /// renderer, so it gets a far tighter budget than <see cref="MaxRenderPixels"/>.
    /// Without it the page keeps whatever the last untiled zoom produced, which
    /// after a jump up from 10% is the <see cref="MinRenderDpi"/> floor —
    /// unreadable mush under a 24x upscale.
    /// </summary>
    public const long BridgeRenderPixels = 2_000_000;

    /// <summary>
    /// The DPI a tiled page's full-page bitmap should be rendered at: its own
    /// target, reduced to fit <see cref="BridgeRenderPixels"/>. Only ever
    /// lowers — a page whose raster already fits keeps exactly what it had, so
    /// this is a no-op for every zoom that does not use tiles.
    /// </summary>
    public static uint BridgeDpi(double pageWidthPt, double pageHeightPt, uint renderDpi)
    {
        if (!IsUsable(pageWidthPt) || !IsUsable(pageHeightPt))
        {
            return renderDpi;
        }

        var pixels = pageWidthPt * renderDpi / 72.0 * (pageHeightPt * renderDpi / 72.0);
        if (pixels <= BridgeRenderPixels)
        {
            return renderDpi;
        }

        // Area scales with the square of DPI, so the reduction does too.
        var reduced = renderDpi * Math.Sqrt(BridgeRenderPixels / pixels);
        return (uint)Math.Clamp(reduced, MinRenderDpi, renderDpi);
    }

    /// <summary>Rungs the zoom in/out controls step through.</summary>
    private static readonly double[] Ladder = [0.10, 0.25, 0.50, 0.75, 1.00, 1.25, 1.50, 2.00, 3.00, 4.00, 6.00, 8.00];

    public static double ClampFactor(double factor) =>
        double.IsFinite(factor) ? Math.Clamp(factor, MinFactor, MaxFactor) : 1.0;

    /// <summary>
    /// Resolves one page's geometry. Page dimensions are in PDF points; the
    /// viewport is in DIPs and may be unmeasured (zero) during the first
    /// layout pass, in which case the fit modes fall back to 100% rather than
    /// collapsing the page.
    /// </summary>
    public static PageBox Resolve(ZoomSetting setting, double pageWidthPt, double pageHeightPt, ViewportSize viewport, double rasterizationScale = 1.0)
    {
        if (!IsUsable(pageWidthPt) || !IsUsable(pageHeightPt))
        {
            // A page without usable dimensions cannot be fitted to anything.
            // Give it a minimal well-formed box so the placeholder, the scroll
            // stacking and the render request all stay valid.
            return new PageBox(1, 1, MinRenderDpi, 1.0);
        }

        var factor = ClampFactor(setting.Mode switch
        {
            PageZoomMode.FitWidth => FitFactor(viewport.WidthDips / pageWidthPt, IsUsable(viewport.WidthDips)),
            PageZoomMode.FitPage => FitFactor(
                Math.Min(viewport.WidthDips / pageWidthPt, viewport.HeightDips / pageHeightPt),
                IsUsable(viewport.WidthDips) && IsUsable(viewport.HeightDips)),
            _ => setting.Factor
        });

        var dipsPerPoint = factor * DipsPerPointAt100;
        return new PageBox(
            pageWidthPt * dipsPerPoint,
            pageHeightPt * dipsPerPoint,
            RenderDpiFor(pageWidthPt, pageHeightPt, dipsPerPoint, rasterizationScale),
            factor);
    }

    /// <summary>Next rung up the ladder, as an explicit zoom.</summary>
    public static ZoomSetting StepIn(double currentFactor)
    {
        var current = ClampFactor(currentFactor);
        foreach (var rung in Ladder)
        {
            if (rung > current + Epsilon)
            {
                return ZoomSetting.Custom(rung);
            }
        }

        return ZoomSetting.Custom(MaxFactor);
    }

    /// <summary>Next rung down the ladder, as an explicit zoom.</summary>
    public static ZoomSetting StepOut(double currentFactor)
    {
        var current = ClampFactor(currentFactor);
        for (var index = Ladder.Length - 1; index >= 0; index--)
        {
            if (Ladder[index] < current - Epsilon)
            {
                return ZoomSetting.Custom(Ladder[index]);
            }
        }

        return ZoomSetting.Custom(MinFactor);
    }

    private const double Epsilon = 1e-9;

    /// <summary>
    /// One rendered pixel per physical pixel of the page's DIP box, then bounded
    /// twice: by the hard DPI ceiling and by the per-page pixel budget. Both
    /// only reduce resolution — the caller's box is already fixed, and the
    /// renderer output is stretched to fill it.
    /// </summary>
    private static uint RenderDpiFor(double pageWidthPt, double pageHeightPt, double dipsPerPoint, double rasterizationScale)
    {
        var scale = IsUsable(rasterizationScale) ? rasterizationScale : 1.0;
        var dpi = Math.Clamp(72.0 * dipsPerPoint * scale, MinRenderDpi, MaxRenderDpi);
        var pixels = pageWidthPt * dpi / 72.0 * (pageHeightPt * dpi / 72.0);
        if (pixels > MaxRenderPixels)
        {
            // Area scales with the square of DPI, so the reduction does too.
            dpi = Math.Max(MinRenderDpi, dpi * Math.Sqrt(MaxRenderPixels / pixels));
        }

        // Truncate rather than round: rounding up can push the page back over
        // the ceiling the reduction was there to enforce.
        return Math.Max(MinRenderDpi, (uint)dpi);
    }

    private static double FitFactor(double dipsPerPoint, bool viewportIsMeasured) =>
        viewportIsMeasured ? dipsPerPoint / DipsPerPointAt100 : 1.0;

    private static bool IsUsable(double value) => double.IsFinite(value) && value > 0;
}
