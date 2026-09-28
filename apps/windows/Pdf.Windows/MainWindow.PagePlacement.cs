using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>
/// Where page-space geometry lands on a page as it is drawn — the one place
/// every overlay (run outlines, the inline editor, selection, search hits,
/// annotations) and every pointer hit-test goes through.
/// </summary>
/// <remarks>
/// A page carrying <c>/Rotate</c> is rendered turned, but nothing on it moved:
/// its text runs, search hits and annotations are all still in the page's
/// unrotated space. A bare y-flip puts every overlay on a turned page where the
/// page would be if it had never been turned. The per-turn arithmetic is the
/// core's (<c>pdf_render::selection</c>, reached through the facade); what
/// lives here is the WinUI end — sizing a <see cref="Canvas"/> child, and
/// turning the ones whose <em>contents</em> must turn with the page.
/// </remarks>
public sealed partial class MainWindow
{
    /// <summary>One page as it is drawn right now: its size, its turn, and the current zoom.</summary>
    private PagePlacement PlacementOf(PageSlot slot, int pageIndex) =>
        PagePlacement.Of(_session!.Pages[pageIndex], slot.Scale);

    /// <summary>A pointer position on a page, back into page space.</summary>
    private AnnotationPoint ToPdf(PageSlot slot, int pageIndex, global::Windows.Foundation.Point point) =>
        _facade.PointToPdf(new PlacedPoint(point.X, point.Y), PlacementOf(slot, pageIndex));

    /// <summary>Where a page-space point lands on the drawn page.</summary>
    private PlacedPoint PlaceOnPage(PageSlot slot, int pageIndex, AnnotationPoint point) =>
        _facade.PlacePoint(point, PlacementOf(slot, pageIndex));

    /// <summary>
    /// Sizes and positions a plain box — an outline, a highlight — over a
    /// page-space rect. A quarter turn swaps its width and height; the box
    /// itself has nothing inside that needs turning.
    /// </summary>
    /// <param name="minSide">
    /// The thinnest the box may get, on either axis: an underline is a 2pt
    /// rect, and which screen axis is its thin one depends on the turn.
    /// </param>
    private void PlaceOverPage(FrameworkElement element, PageSlot slot, int pageIndex, AnnotationRect rect, double minSide = 1)
    {
        var placed = _facade.PlaceRect(rect, PlacementOf(slot, pageIndex));
        element.Width = Math.Max(minSide, placed.Width);
        element.Height = Math.Max(minSide, placed.Height);
        Canvas.SetLeft(element, placed.Left);
        Canvas.SetTop(element, placed.Top);
    }

    /// <summary>
    /// Positions an element laid out <em>upright</em> — sized in the rect's own
    /// unturned frame — and turns it with the page, for content that has a
    /// reading direction: the inline editor's text, a stamp's picture.
    /// </summary>
    /// <remarks>
    /// The element turns about its own top-left, which is pinned to where the
    /// rect's upright top-left corner lands on the drawn page. An element that
    /// does not start at that corner (the editor's text box sits a baseline
    /// below it) gives its offset in the upright frame, and the offset is
    /// turned with it — on a page turned 90° clockwise, "down" points left.
    /// </remarks>
    private void PlaceUpright(FrameworkElement element, PageSlot slot, int pageIndex, AnnotationRect rect, double offsetX = 0, double offsetY = 0)
    {
        var page = PlacementOf(slot, pageIndex);
        var anchor = _facade.PlacePoint(new AnnotationPoint(rect.X, rect.Y + rect.Height), page);
        var radians = page.Degrees * Math.PI / 180;
        var (cos, sin) = (Math.Cos(radians), Math.Sin(radians));
        Canvas.SetLeft(element, anchor.Left + (offsetX * cos) - (offsetY * sin));
        Canvas.SetTop(element, anchor.Top + (offsetX * sin) + (offsetY * cos));
        element.RenderTransformOrigin = default;
        element.RenderTransform = page.Rotation == PageRotation.None ? null : new RotateTransform { Angle = page.Degrees };
    }
}
