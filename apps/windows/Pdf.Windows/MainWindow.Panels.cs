using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Windows.System;

namespace Pdf.Windows;

/// <summary>Window-local sidebar visibility and width; no document edits.</summary>
public sealed partial class MainWindow
{
    private const double SidebarMinimumWidth = 300;
    private const double SidebarMaximumWidth = 600;
    private const double ViewerMinimumWidth = 240;
    private const double SidebarDividerWidth = 16;
    private double _preferredSidebarWidth = SidebarMinimumWidth;

    private void PanelsButton_Click(object sender, RoutedEventArgs e) => SetDocumentPanelsVisible(PanelsButton.IsChecked == true);

    private void SetDocumentPanelsVisible(bool visible)
    {
        PanelsButton.IsChecked = visible;
        DocumentSidebar.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
        SidebarDivider.Visibility = DocumentSidebar.Visibility;
        UpdateSidebarWidth();
        // Auto columns release both the panel and divider when collapsed.
        // PageScroller_SizeChanged retargets fit zoom for the new viewport.
    }

    private void DocumentArea_SizeChanged(object sender, SizeChangedEventArgs e) => UpdateSidebarWidth();

    private void UpdateSidebarWidth()
    {
        // Preserve the requested width through a temporary smaller window.
        // At very narrow widths the panel's controls still need their minimum.
        var maximum = Math.Clamp(DocumentArea.ActualWidth - SidebarDividerWidth - ViewerMinimumWidth,
            SidebarMinimumWidth, SidebarMaximumWidth);
        DocumentSidebar.Width = Math.Min(_preferredSidebarWidth, maximum);
    }

    private void ResizeSidebar(double delta)
    {
        _preferredSidebarWidth = Math.Clamp(DocumentSidebar.Width + delta, SidebarMinimumWidth, SidebarMaximumWidth);
        UpdateSidebarWidth();
    }

    private void SidebarDivider_DragDelta(object sender, DragDeltaEventArgs e) => ResizeSidebar(-e.HorizontalChange);

    private void SidebarDivider_KeyDown(object sender, KeyRoutedEventArgs e)
    {
        // Moving the divider left widens the right-hand panel.
        switch (e.Key)
        {
            case VirtualKey.Left:
                ResizeSidebar(16);
                break;
            case VirtualKey.Right:
                ResizeSidebar(-16);
                break;
            default:
                return;
        }

        e.Handled = true;
    }
}
