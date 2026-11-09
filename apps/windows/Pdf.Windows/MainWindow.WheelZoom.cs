using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Viewer;
using Windows.System;
using Windows.Foundation;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private void InitializeWheelZoom()
    {
        // ScrollViewer may mark wheel input handled even with native zoom disabled.
        // Keep PDF zoom on ApplyZoom so geometry, overlays and render DPI agree.
        PageScroller.AddHandler(UIElement.PointerWheelChangedEvent,
            new PointerEventHandler(PageScroller_PointerWheelChanged), handledEventsToo: true);
    }

    private void PageScroller_PointerWheelChanged(object sender, PointerRoutedEventArgs e)
    {
        if (!e.KeyModifiers.HasFlag(VirtualKeyModifiers.Control)) return;
        var properties = e.GetCurrentPoint(PageScroller).Properties;
        if (properties.IsHorizontalMouseWheel || properties.MouseWheelDelta == 0) return;
        if (_isBusy || _dialogOpen || _organizing || _shellPickingFile
            || _session is not { PageCount: > 0 } || _slots.Count == 0
            || PageScroller.Visibility != Visibility.Visible) return;

        e.Handled = true;
        var viewport = e.GetCurrentPoint(PageScroller).Position;
        var index = Math.Max(0, _spans.FindLastIndex(span => span.Top <= PageScroller.VerticalOffset + viewport.Y));
        var page = _slots[index].Container;
        if (page.ActualWidth <= 0 || page.ActualHeight <= 0) return;
        var local = e.GetCurrentPoint(page).Position;
        var anchor = new WheelZoomAnchor(index,
            new Point(local.X / page.ActualWidth, local.Y / page.ActualHeight), viewport);
        ApplyZoom(properties.MouseWheelDelta > 0
            ? PageZoom.StepIn(_slots[index].Factor)
            : PageZoom.StepOut(_slots[index].Factor), anchor);
    }

    private readonly record struct WheelZoomAnchor(int PageIndex, Point Fraction, Point Viewport);

    private void RestoreWheelZoomAnchor(WheelZoomAnchor anchor)
    {
        var page = _slots[anchor.PageIndex].Container;
        var position = page.TransformToVisual(PageScroller).TransformPoint(
            new Point(anchor.Fraction.X * page.ActualWidth, anchor.Fraction.Y * page.ActualHeight));
        // Native layout includes page centering and fixed inter-page gaps. Only
        // the page-local point scales; a document-wide offset ratio would drift.
        PageScroller.ChangeView(
            PageScroller.HorizontalOffset + position.X - anchor.Viewport.X,
            PageScroller.VerticalOffset + position.Y - anchor.Viewport.Y,
            null, disableAnimation: true);
    }
}
