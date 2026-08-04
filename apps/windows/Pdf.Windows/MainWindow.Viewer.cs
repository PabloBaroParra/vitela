using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// The continuous page viewer: zoom, placeholder geometry, the visible-range
/// walk that drives lazy rendering, and bitmap eviction outside the keep
/// window. The arithmetic lives in <see cref="PageZoom"/>; this partial owns
/// only the chrome and the scroll bookkeeping.
/// </summary>
public sealed partial class MainWindow
{
    private const double PageSpacing = 12;
    /// <summary>Rendered pages kept alive beyond the visible range, per side.</summary>
    private const int KeepWindow = 2;
    /// <summary>Pages rendered ahead of the visible range, per side.</summary>
    private const int PrefetchWindow = 1;
    /// <summary>
    /// Page border plus breathing room, subtracted from the viewport before
    /// fitting so a fit-width page never trips the horizontal scrollbar.
    /// </summary>
    private const double PageChromeDips = 16;

    private ZoomSetting _zoom = ZoomSetting.FitWidth;
    private List<PageSlot> _slots = [];
    /// <summary>Page geometry at the current zoom — what the viewport walk reads.</summary>
    private List<PageSpan> _spans = [];
    /// <summary>Page under the top of the viewport, as of the last walk.</summary>
    private int _firstVisiblePage;
    private XamlRoot? _xamlRoot;
    private double _rasterizationScale = 1.0;
    /// <summary>
    /// The pages allowed to hold a render right now. A render that finishes
    /// after the zoom or scroll moved consults this before asking again, so
    /// work is never re-queued for a page that has left the screen.
    /// </summary>
    private PageWindow _renderWindow = PageWindow.Empty;
    /// <summary>
    /// A DPI retarget is visible work first: prefetch resumes only after the
    /// viewport has caught up to its current target.
    /// </summary>
    private bool _deferPrefetchUntilVisibleSettles;

    private void PageScroller_ViewChanged(object? sender, ScrollViewerViewChangedEventArgs e)
    {
        UpdateViewport(e.IsIntermediate);
    }

    private void XamlRoot_Changed(XamlRoot sender, XamlRootChangedEventArgs args)
    {
        if (sender.RasterizationScale == _rasterizationScale || _session is null || _slots.Count == 0)
        {
            return;
        }

        ApplyZoom(_zoom);
    }

    private void PageScroller_SizeChanged(object sender, SizeChangedEventArgs e)
    {
        // A fit mode is defined by the viewport, so resizing it changes the
        // page boxes. An explicit zoom is not, and must not move.
        if (_session is not null && _slots.Count > 0 && _zoom.Mode != PageZoomMode.Custom)
        {
            ApplyZoom(_zoom);
            return;
        }

        UpdateViewport(intermediate: false);
    }

    private void ZoomInButton_Click(object sender, RoutedEventArgs e) => ApplyZoom(PageZoom.StepIn(CurrentFactor()));

    private void ZoomOutButton_Click(object sender, RoutedEventArgs e) => ApplyZoom(PageZoom.StepOut(CurrentFactor()));

    private void FitWidthButton_Click(object sender, RoutedEventArgs e) => ApplyZoom(ZoomSetting.FitWidth);

    private void FitPageButton_Click(object sender, RoutedEventArgs e) => ApplyZoom(ZoomSetting.FitPage);

    /// <summary>
    /// Publishes a freshly opened document: one placeholder per page, then a
    /// second layout pass once the scroller is visible. The first pass runs
    /// against a collapsed (zero-sized) viewport, which the fit modes cannot
    /// use.
    /// </summary>
    private void ShowDocumentPages(DocumentSession session)
    {
        BuildPagePlaceholders(session);
        PageScroller.Visibility = Visibility.Visible;
        PageScroller.ChangeView(0, 0, 1, disableAnimation: true);
        PageScroller.UpdateLayout();
        LayoutPages(session);
        PageScroller.UpdateLayout();
        UpdateViewport(intermediate: false);
    }

    /// <summary>
    /// Re-lays out at a new zoom, keeping the reader where they were: the
    /// page under the top of the viewport, and how far into it they had
    /// scrolled, both survive the change.
    /// </summary>
    private void ApplyZoom(ZoomSetting zoom)
    {
        _zoom = zoom;
        if (_session is null || _slots.Count == 0)
        {
            return;
        }

        var anchor = CaptureAnchor();
        if (LayoutPages(_session))
        {
            _deferPrefetchUntilVisibleSettles = true;
        }
        PageScroller.UpdateLayout();
        var span = _spans[Math.Clamp(anchor.PageIndex, 0, _spans.Count - 1)];
        PageScroller.ChangeView(null, span.Top + (anchor.Fraction * span.Height), null, disableAnimation: true);
        UpdateViewport(intermediate: false);
        // Annotation visuals are sized in screen pixels from `slot.Scale`, which
        // `LayoutPages` just moved. Unlike the page bitmap — which the Image
        // stretches into its new box on its own — the overlay is rebuilt from
        // PDF points on every paint, so without this the previous zoom's
        // geometry stays on screen until some unrelated edit repaints it.
        RedrawAnnotations();
    }

    /// <summary>
    /// One entry per page. The slot owns the visual tree; its geometry is
    /// filled in by <see cref="LayoutPages"/> and refreshed on every zoom.
    /// </summary>
    private void BuildPagePlaceholders(DocumentSession session)
    {
        PageStack.Children.Clear();
        PageStack.Spacing = PageSpacing;
        _slots = new List<PageSlot>((int)session.PageCount);

        foreach (var _ in session.Pages)
        {
            var image = new Image { Stretch = Stretch.Fill };
            var tiles = new Canvas { IsHitTestVisible = false };
            var highlights = new Canvas { Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent) };
            var pageLayer = new Grid();
            pageLayer.Children.Add(image);
            pageLayer.Children.Add(tiles);
            pageLayer.Children.Add(highlights);
            var container = new Border
            {
                BorderThickness = new Thickness(1),
                BorderBrush = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                // Unrendered pages read as paper, not as a hole in the theme.
                Background = new SolidColorBrush(Microsoft.UI.Colors.White),
                Child = pageLayer,
            };
            var slot = new PageSlot(container, image, tiles, highlights);
            ConnectAnnotationPointer(slot, _slots.Count);
            _slots.Add(slot);
            PageStack.Children.Add(container);
        }

        LayoutPages(session);
    }

    /// <summary>
    /// Resolves every page box at the current zoom and restacks the tops that
    /// <see cref="UpdateViewport"/> walks.
    ///
    /// Bitmaps are deliberately NOT cleared here. The page's <see cref="Image"/>
    /// stretches whatever it already has into the new box, so a zoom change is
    /// visible immediately and merely soft until the sharper render lands.
    /// Clearing instead would blank every visible page for a full render
    /// round-trip.
    /// </summary>
    private bool LayoutPages(DocumentSession session)
    {
        var viewport = new ViewportSize(
            PageScroller.ViewportWidth - PageChromeDips,
            PageScroller.ViewportHeight - PageChromeDips);
        var rasterizationScale = _xamlRoot?.RasterizationScale ?? 1.0;

        var retargeted = false;
        _spans = new List<PageSpan>(_slots.Count);
        double top = 0;
        for (var index = 0; index < _slots.Count && index < session.Pages.Count; index++)
        {
            var page = session.Pages[index];
            var box = PageZoom.Resolve(_zoom, page.WidthPt, page.HeightPt, viewport, rasterizationScale);
            var slot = _slots[index];
            slot.Container.Width = box.WidthDips;
            slot.Container.Height = box.HeightDips;
            slot.Scale = box.Factor * PageZoom.DipsPerPointAt100;
            slot.Factor = box.Factor;
            slot.Box = box;
            // A tiled page's full-page bitmap is only ever seen stretched, so
            // it is rendered as a cheap bridge instead of at the page's own
            // target. Every other page keeps that target untouched.
            var baseDpi = ViewportTilePlan.WouldUseTiles(box, rasterizationScale)
                ? PageZoom.BridgeDpi(page.WidthPt, page.HeightPt, box.RenderDpi)
                : box.RenderDpi;
            retargeted |= slot.Render.TargetDpi != baseDpi;
            if (slot.Render.TargetDpi != baseDpi)
            {
                slot.Tiles.Clear();
            }
            slot.Render.RetargetTo(baseDpi);
            _spans.Add(new PageSpan(top, box.HeightDips));
            top += box.HeightDips + PageSpacing;
        }

        _rasterizationScale = rasterizationScale;
        return retargeted;
    }

    private void UpdateViewport(bool intermediate)
    {
        if (_session is null || _slots.Count == 0)
        {
            return;
        }

        var visible = PageWindow.Resolve(_spans, PageScroller.VerticalOffset, PageScroller.ViewportHeight);
        if (_deferPrefetchUntilVisibleSettles && !visible.HasOutstandingRender(index => _slots[index].Render.NeedsRender))
        {
            _deferPrefetchUntilVisibleSettles = false;
        }

        _renderWindow = _deferPrefetchUntilVisibleSettles
            ? visible
            : visible.Expand(PrefetchWindow, _slots.Count);
        _firstVisiblePage = visible.First;
        PageCounter.Text = $"Page {visible.First + 1} of {_slots.Count}";
        ZoomLevel.Text = DescribeZoom(_slots[visible.First].Factor);

        // Request renders even mid-scroll: the facade coalesces per-page
        // requests, and starting early is what makes pages arrive in time.
        var sessionId = _session.SessionId;
        for (var index = _renderWindow.First; index <= _renderWindow.Last; index++)
        {
            RequestRender(sessionId, index);
        }

        // Evict only once the scroll settles, so flinging past pages does
        // not churn bitmaps that are about to come back.
        if (intermediate)
        {
            return;
        }

        var keep = visible.Expand(KeepWindow, _slots.Count);
        for (var index = 0; index < _slots.Count; index++)
        {
            if (keep.Contains(index))
            {
                continue;
            }

            var slot = _slots[index];
            if (slot.Render.HasBitmap && !slot.Render.Requested)
            {
                slot.Image.Source = null;
                slot.Render.DropBitmap();
            }
        }
    }

    private void RequestRender(string sessionId, int pageIndex)
    {
        var slot = _slots[pageIndex];
        var tilePlan = ResolveTilePlan(pageIndex, slot);
        if (tilePlan.UsesTiles && pageIndex == _firstVisiblePage)
        {
            // A tile is visible work only. Do not turn the prefetch window into
            // a competing queue of deep-zoom rasters.
            //
            // Requested before the base render below, deliberately: the two run
            // in separate facade lanes but share one renderer, and the tiles are
            // what the reader is actually looking at.
            RequestVisibleTiles(sessionId, pageIndex, slot, tilePlan);
        }

        if (!slot.Render.ShouldRequest)
        {
            return;
        }

        slot.Render.MarkRequested();
        _ = RenderSlotAsync(sessionId, pageIndex, slot.Render.TargetDpi);
    }

    private ViewportTilePlan ResolveTilePlan(int pageIndex, PageSlot slot)
    {
        var page = _session!.Pages[pageIndex];
        var span = _spans[pageIndex];
        // The page's own box, not the (possibly bridged) base render target —
        // the tile decision must not feed on its own output.
        return ViewportTilePlan.Resolve(
            page.WidthPt,
            page.HeightPt,
            slot.Box,
            new ViewportRect(PageScroller.HorizontalOffset, Math.Max(0, PageScroller.VerticalOffset - span.Top), PageScroller.ViewportWidth, PageScroller.ViewportHeight),
            _rasterizationScale);
    }

    /// <summary>
    /// Asks for every tile the viewport is still missing in one request. Tiles
    /// used to be fetched one at a time, which paid a page load and a full
    /// round-trip per tile; covering one deep-zoom screen took as many
    /// sequential renders as it had tiles.
    /// </summary>
    private void RequestVisibleTiles(string sessionId, int pageIndex, PageSlot slot, ViewportTilePlan plan)
    {
        slot.Tiles.Retarget(plan, slot.Factor);
        if (slot.Tiles.Requested)
        {
            return;
        }

        var missing = slot.Tiles.Missing(plan.VisibleTiles);
        if (missing.Count == 0)
        {
            return;
        }

        slot.Tiles.Requested = true;
        _ = RenderTilesAsync(sessionId, pageIndex, plan.Dpi, missing, slot.Tiles.Generation);
    }

    private async Task RenderTilesAsync(string sessionId, int pageIndex, uint dpi, IReadOnlyList<TileRequest> tiles, ulong generation)
    {
        var regions = tiles
            .Select(tile => new PageRegion((uint)tile.LeftPx, (uint)tile.TopPx, (uint)tile.WidthPx, (uint)tile.HeightPx))
            .ToList();
        var result = await _facade.RenderPageTilesAsync(sessionId, (uint)pageIndex, dpi, regions, false);
        if (_session?.SessionId != sessionId || pageIndex >= _slots.Count)
        {
            return;
        }

        var slot = _slots[pageIndex];
        slot.Tiles.Requested = false;
        if (!result.IsSuccess || generation != slot.Tiles.Generation)
        {
            return;
        }

        var rendered = result.Value!;
        var bitmaps = new WriteableBitmap[rendered.Count];
        for (var index = 0; index < rendered.Count; index++)
        {
            bitmaps[index] = await MaterializeBitmapAsync(rendered[index]);
        }

        // Re-checked after materializing: the zoom may have moved while the
        // batch was being swizzled, and these tiles are placed with the scale
        // that produced them.
        if (_session?.SessionId != sessionId || generation != slot.Tiles.Generation)
        {
            return;
        }

        var dipPerPixel = slot.Factor * 96.0 / dpi;
        for (var index = 0; index < rendered.Count && index < tiles.Count; index++)
        {
            var tile = tiles[index];
            var image = new Image { Source = bitmaps[index], Stretch = Stretch.Fill };
            Canvas.SetLeft(image, tile.LeftPx * dipPerPixel);
            Canvas.SetTop(image, tile.TopPx * dipPerPixel);
            image.Width = tile.WidthPx * dipPerPixel;
            image.Height = tile.HeightPx * dipPerPixel;
            slot.Tiles.Add(tile, image);
        }

        UpdateViewport(intermediate: false);
    }

    private async Task RenderSlotAsync(string sessionId, int pageIndex, uint dpi)
    {
        var result = await _facade.RenderPageAsync(sessionId, (uint)pageIndex, dpi, false);
        if (_session?.SessionId != sessionId || pageIndex >= _slots.Count)
        {
            return;
        }

        var slot = _slots[pageIndex];
        if (result.IsDiscarded || result.IsEmpty)
        {
            slot.Render.Fail();
            return;
        }

        if (!result.IsSuccess)
        {
            slot.Render.Fail();
            // A single failed page keeps its placeholder; only fail the whole
            // view when nothing has rendered at all (e.g. a broken document).
            if (!_slots.Any(other => other.Render.HasBitmap))
            {
                ShowError(result.Error!);
            }

            return;
        }

        if (slot.Render.DiscardIfSuperseded(dpi))
        {
            if (_renderWindow.Contains(pageIndex))
            {
                RequestRender(sessionId, pageIndex);
            }

            return;
        }

        // The page stays marked in-flight across materialization, so a viewport
        // walk in the meantime cannot queue a duplicate render for it.
        var bitmap = await MaterializeBitmapAsync(result.Value!);
        if (_session?.SessionId != sessionId || pageIndex >= _slots.Count)
        {
            return;
        }

        if (slot.Render.CompleteWith(dpi))
        {
            slot.Image.Source = bitmap;
            UpdateViewport(intermediate: false);
            return;
        }

        // The zoom moved while this page was rendering, so the bitmap that just
        // arrived is stale. Ask again only if the page is still on screen:
        // zooming out then in supersedes a whole screenful of renders at once,
        // and re-queueing them all at the new, far higher DPI would bury the
        // one page the reader is actually waiting on. A page outside the window
        // keeps whatever it had and is picked up again when it scrolls back.
        if (_renderWindow.Contains(pageIndex))
        {
            RequestRender(sessionId, pageIndex);
        }
    }

    /// <summary>
    /// The zoom the reader is actually looking at. In a fit mode that is the
    /// first visible page's resolved factor, so stepping in or out continues
    /// from what is on screen rather than from an abstract 100%.
    /// </summary>
    private double CurrentFactor() =>
        _slots.Count > 0 ? _slots[Math.Clamp(_firstVisiblePage, 0, _slots.Count - 1)].Factor : 1.0;

    private string DescribeZoom(double factor) => _zoom.Mode switch
    {
        PageZoomMode.FitWidth => $"Fit width ({factor * 100:F0}%)",
        PageZoomMode.FitPage => $"Fit page ({factor * 100:F0}%)",
        _ => $"{factor * 100:F0}%"
    };

    private ScrollAnchor CaptureAnchor()
    {
        var viewportTop = PageScroller.VerticalOffset;
        var index = Math.Max(0, _spans.FindLastIndex(span => span.Top <= viewportTop));
        var span = _spans[index];
        var fraction = span.Height > 0 ? Math.Clamp((viewportTop - span.Top) / span.Height, 0, 1) : 0;
        return new ScrollAnchor(index, fraction);
    }

    /// <summary>Where the reader was, in page-relative terms a zoom cannot invalidate.</summary>
    private readonly record struct ScrollAnchor(int PageIndex, double Fraction);

    private sealed class PageSlot(Border container, Image image, Canvas tiles, Canvas highlights)
    {
        public Border Container { get; } = container;
        public Image Image { get; } = image;
        public Canvas TileCanvas { get; } = tiles;
        public Canvas Highlights { get; } = highlights;
        /// <summary>DIPs per PDF point at the current zoom — how overlays map page geometry.</summary>
        public double Scale { get; set; }
        public double Factor { get; set; }
        /// <summary>The page's resolved geometry, before any bridge reduction.</summary>
        public PageBox Box { get; set; }
        /// <summary>What this page owes the renderer, and what it already shows.</summary>
        public PageRenderPlan Render { get; } = new();
        public TileCache Tiles { get; } = new(tiles);
    }

    private sealed class TileCache(Canvas canvas)
    {
        /// <summary>Tiles kept beyond the visible set, so a small scroll back is free.</summary>
        private const int Slack = 2;
        private readonly Queue<(TileRequest Tile, Image Image)> _images = [];
        private readonly HashSet<TileRequest> _tiles = [];
        /// <summary>
        /// Never below the visible tile count. A smaller ceiling would evict a
        /// tile the viewport still needs, which the next walk would immediately
        /// re-request — rendering forever without ever completing the screen.
        /// </summary>
        private int _capacity = Slack;
        private uint _dpi;
        private double _factor;
        public bool Requested { get; set; }
        public ulong Generation { get; private set; }

        public void Add(TileRequest tile, Image image)
        {
            canvas.Children.Add(image);
            _images.Enqueue((tile, image));
            _tiles.Add(tile);
            while (_images.Count > _capacity)
            {
                var evicted = _images.Dequeue();
                _tiles.Remove(evicted.Tile);
                canvas.Children.Remove(evicted.Image);
            }
        }

        /// <summary>The visible tiles this cache does not already hold, in viewport order.</summary>
        public List<TileRequest> Missing(IReadOnlyList<TileRequest> candidates) =>
            [.. candidates.Where(candidate => !_tiles.Contains(candidate))];

        /// <summary>
        /// Only a change of render scale invalidates what is on the canvas:
        /// tiles are placed from their page-pixel origin, so scrolling to a new
        /// tile row leaves every tile already rendered exactly where it belongs.
        /// Discarding them there would drop the reader back to the stretched
        /// base bitmap and orphan the batch still in flight.
        ///
        /// The zoom factor is checked alongside the DPI because they can move
        /// independently: once the DPI ceiling is reached, 600% and 800% render
        /// at the same DPI but lay out at different sizes, and a tile placed for
        /// one would sit in the wrong place under the other.
        /// </summary>
        public void Retarget(ViewportTilePlan plan, double factor)
        {
            if (_dpi != plan.Dpi || _factor != factor)
            {
                Clear();
                _dpi = plan.Dpi;
                _factor = factor;
            }

            _capacity = plan.VisibleTiles.Count + Slack;
        }

        public void Clear()
        {
            Generation++;
            Requested = false;
            _images.Clear();
            _tiles.Clear();
            _capacity = Slack;
            _dpi = 0;
            _factor = 0;
            canvas.Children.Clear();
        }
    }
}
