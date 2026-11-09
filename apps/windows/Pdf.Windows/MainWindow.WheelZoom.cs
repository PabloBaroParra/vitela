using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Viewer;
using Windows.System;

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
        ApplyZoom(properties.MouseWheelDelta > 0
            ? PageZoom.StepIn(CurrentFactor())
            : PageZoom.StepOut(CurrentFactor()));
    }
}
