using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Viewer;
using Windows.System;

namespace Pdf.Windows;

/// <summary>
/// Window-local editor columns: Pages on the left, Tools on the right, each
/// with its own divider and toolbar toggle. No document edits.
/// </summary>
public sealed partial class MainWindow
{
    private const double ViewerMinimumWidth = 240;
    private const double DividerWidth = 16;
    private const double DividerKeyStep = 16;
    private const double ToolsSqueezeFloor = 200;

    private readonly SideColumn _pagesColumn = new(minimum: 120, maximum: 320, initial: 160);
    private readonly SideColumn _toolsColumn = new(minimum: 300, maximum: 600, initial: 300);

    private void PagesPanelButton_Click(object sender, RoutedEventArgs e) =>
        SetPagesPanelVisible(PagesPanelButton.IsChecked == true);

    private void ToolsPanelButton_Click(object sender, RoutedEventArgs e) =>
        SetToolsPanelVisible(ToolsPanelButton.IsChecked == true);

    private void SetPagesPanelVisible(bool visible)
    {
        _pagesColumn.SetOpen(visible);
        UpdateSideColumns();
    }

    /// <summary>Search results and every tool page live in the Tools column.</summary>
    private void SetToolsPanelVisible(bool visible)
    {
        _toolsColumn.SetOpen(visible);
        UpdateSideColumns();
    }

    private void DocumentArea_SizeChanged(object sender, SizeChangedEventArgs e) => UpdateSideColumns();

    /// <summary>
    /// Lays both columns out from their state. The canvas absorbs a window
    /// resize; Tools is sized first so a narrow window squeezes Pages before it
    /// squeezes the controls. The dividers stay on screen while a column is
    /// folded, so dragging back is how the column reopens.
    /// PageScroller_SizeChanged retargets fit zoom for the new viewport.
    /// </summary>
    private void UpdateSideColumns()
    {
        var room = DocumentArea.ActualWidth - 2 * DividerWidth - ViewerMinimumWidth;
        var pagesFloor = _pagesColumn.IsOpen ? _pagesColumn.Minimum : 0;
        // Tools keeps priority and may squeeze (its rows wrap); Pages folds when it no longer fits.
        var tools = _toolsColumn.Width(room - pagesFloor, ToolsSqueezeFloor);
        var pages = _pagesColumn.WidthOrFold(room - tools);
        ApplyColumn(DocumentSidebar, ToolsPanelButton, _toolsColumn, tools);
        ApplyColumn(PagesSidebar, PagesPanelButton, _pagesColumn, pages);
    }

    private static void ApplyColumn(FrameworkElement panel, ToggleButton toggle, SideColumn column, double width)
    {
        toggle.IsChecked = column.IsOpen;
        // Open by choice but folded for lack of room reads as collapsed; the toggle keeps the choice.
        var shown = column.IsOpen && width > 0;
        panel.Visibility = shown ? Visibility.Visible : Visibility.Collapsed;
        if (shown) panel.Width = width;
    }

    private void PagesDivider_DragStarted(object sender, DragStartedEventArgs e) => _pagesColumn.BeginDrag();

    private void ToolsDivider_DragStarted(object sender, DragStartedEventArgs e) => _toolsColumn.BeginDrag();

    // Pages sits left of its divider, so dragging right widens it; Tools is the mirror image.
    private void PagesDivider_DragDelta(object sender, DragDeltaEventArgs e) => DragColumn(_pagesColumn, e.HorizontalChange);

    private void ToolsDivider_DragDelta(object sender, DragDeltaEventArgs e) => DragColumn(_toolsColumn, -e.HorizontalChange);

    private void PagesDivider_KeyDown(object sender, KeyRoutedEventArgs e) => StepColumn(_pagesColumn, e, widenKey: VirtualKey.Right);

    private void ToolsDivider_KeyDown(object sender, KeyRoutedEventArgs e) => StepColumn(_toolsColumn, e, widenKey: VirtualKey.Left);

    private void DragColumn(SideColumn column, double widthDelta)
    {
        column.DragBy(widthDelta);
        UpdateSideColumns();
    }

    /// <summary>The keyboard path of a divider drag: the same fold/unfold rule, one step at a time.</summary>
    private void StepColumn(SideColumn column, KeyRoutedEventArgs e, VirtualKey widenKey)
    {
        if (e.Key is not (VirtualKey.Left or VirtualKey.Right)) return;
        column.BeginDrag();
        DragColumn(column, e.Key == widenKey ? DividerKeyStep : -DividerKeyStep);
        e.Handled = true;
    }
}
