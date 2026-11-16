using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// The floating quick-action bar beside whatever Edit content has selected —
/// the Linux shell's <c>content_edit::quick_actions</c> (PR #567): retype a
/// text block, replace an image, cut, copy and delete.
/// </summary>
/// <remarks>
/// Every button calls the handler its existing door calls — F2, the Images
/// card's Replace, Ctrl+X, Ctrl+C, the Delete key — and may be pressed under
/// the gate that door uses (<see cref="ContentQuickActions.Offer"/>). The bar
/// sits on a hit-transparent canvas over the page view rather than in a
/// flyout: it never takes focus or a light-dismiss layer, and where it goes is
/// <see cref="ContentQuickActions.BarOrigin"/>'s arithmetic. It follows the
/// selection through scrolling and zoom by re-placing itself on an idle after
/// any of them, and hides while a press is dragged, an inline editor is open,
/// the selection clears or the mode goes off.
/// </remarks>
public sealed partial class MainWindow
{
    private readonly Button _quickRetype = new();
    private readonly Button _quickReplace = new();
    private readonly Button _quickCut = new();
    private readonly Button _quickCopy = new();
    private readonly Button _quickDelete = new();

    /// <summary>Whether a re-placement already waits on an idle, so a burst of redraws costs one.</summary>
    private bool _quickActionsScheduled;

    private void BuildQuickActions()
    {
        ToolbarIcon(_quickRetype, ShellIcon.Text, "Edit text", "Retype the selected text in place (F2)", "Edit text", EditIconSize, IconTint.Accent);
        ToolbarIcon(_quickReplace, ShellIcon.Image, "Replace", "Swap the selected image for a PNG or JPEG", "Replace", EditIconSize, IconTint.Accent);
        // No shell icon says "cut" or "copy", so these two are words alone.
        _quickCut.Content = "Cut";
        AutomationProperties.SetName(_quickCut, "Cut");
        ToolTipService.SetToolTip(_quickCut, "Cut the selection to the clipboard (Ctrl+X)");
        _quickCopy.Content = "Copy";
        AutomationProperties.SetName(_quickCopy, "Copy");
        ToolTipService.SetToolTip(_quickCopy, "Copy the selection to the clipboard (Ctrl+C)");
        ToolbarIcon(_quickDelete, ShellIcon.Delete, "Delete", "Remove the selection from the page (Delete)", "Delete", EditIconSize, IconTint.Accent);
        foreach (var button in new[] { _quickRetype, _quickReplace, _quickCut, _quickCopy, _quickDelete })
        {
            button.Style = NamedStyle("EditTileButtonStyle");
            button.Padding = new Thickness(8, 3, 8, 3);
            button.CornerRadius = new CornerRadius(7);
            // A click on the bar leaves the keyboard where it was: Delete, F2
            // and the clipboard keys stand down while a text field has it.
            button.AllowFocusOnInteraction = false;
            QuickActionsButtons.Children.Add(button);
        }

        _quickRetype.Click += async (_, _) => await RetypeSelectedTextBlockAsync();
        _quickReplace.Click += ReplaceImageButton_Click;
        _quickCut.Click += (_, _) =>
        {
            if (ContentClipboardReady && ClipboardSelection() is { } selection) _ = CopyContentAsync(selection, cut: true);
        };
        _quickCopy.Click += (_, _) => TryCopyContent();
        _quickDelete.Click += async (_, _) =>
        {
            if (_contentEditMode && _pump.Box is null && SelectedContentDeletion() is { } deletion) await deletion();
        };
    }

    private void QuickActionsLayer_SizeChanged(object sender, SizeChangedEventArgs e) => ScheduleQuickActions();

    /// <summary>Re-places the bar once the page view is laid out again, coalescing every request made before then.</summary>
    private void ScheduleQuickActions()
    {
        if (_quickActionsScheduled) return;
        _quickActionsScheduled = true;
        DispatcherQueue.TryEnqueue(DispatcherQueuePriority.Low, () =>
        {
            _quickActionsScheduled = false;
            RefreshQuickActions();
        });
    }

    /// <summary>What is selected, the page it is on and its box — in the order the Delete key asks.</summary>
    private (QuickActionsSelection Kind, uint PageIndex, AnnotationRect Bounds)? QuickActionsSelected()
    {
        if (_contentSelection is { IsEmpty: false } selection && selection.Bounds(DrawnBounds) is { } box)
            return (QuickActionsSelection.Several, selection.PageIndex, box);
        if (_selectedTextBlock is { } block) return (QuickActionsSelection.TextBlock, block.PageIndex, DrawnBounds(block));
        if (_selectedContentImage is { } image) return (QuickActionsSelection.Image, image.PageIndex, image.Bounds);
        if (_selectedContentPath is { } path) return (QuickActionsSelection.Shape, path.PageIndex, _pathEditPreview ?? path.Bounds);
        return null;
    }

    /// <summary>Shows the bar for the selection as it stands, where it belongs, or hides it.</summary>
    private void RefreshQuickActions()
    {
        var bar = QuickActionsBar;
        var shown = ContentQuickActions.MayShow(_contentEditMode && _session is not null, _contentInsertKind is not null,
            ContentDragging, ContentEditorOpen, _organizing, _dialogOpen);
        if (!shown || QuickActionsSelected() is not { } selected || PlaceQuickActions(selected) is not { } origin)
        {
            bar.Visibility = Visibility.Collapsed;
            return;
        }
        Canvas.SetLeft(bar, Math.Round(origin.X));
        Canvas.SetTop(bar, Math.Round(origin.Y));
    }

    /// <summary>
    /// Spends the offer on the buttons, shows the bar and returns where it
    /// goes on the layer — or <c>null</c> when the selection is out of view.
    /// </summary>
    private (double X, double Y)? PlaceQuickActions((QuickActionsSelection Kind, uint PageIndex, AnnotationRect Bounds) selected)
    {
        if (_session is null || selected.PageIndex >= _slots.Count || selected.PageIndex >= _session.Pages.Count) return null;
        var offer = ContentQuickActions.Offer(selected.Kind, ContentClipboardReady, ContentEditButton.IsEnabled,
            ReplaceImageButton.IsEnabled, DeleteImageButton.IsEnabled, _deleteShapeButton.IsEnabled);
        _quickRetype.Visibility = offer.Retype is null ? Visibility.Collapsed : Visibility.Visible;
        _quickRetype.IsEnabled = offer.Retype == true;
        _quickReplace.Visibility = offer.Replace is null ? Visibility.Collapsed : Visibility.Visible;
        _quickReplace.IsEnabled = offer.Replace == true;
        _quickCut.IsEnabled = offer.Cut;
        _quickCopy.IsEnabled = offer.Copy;
        _quickDelete.IsEnabled = offer.Delete;

        var slot = _slots[(int)selected.PageIndex];
        var placed = _facade.PlaceRect(selected.Bounds, PlacementOf(slot, (int)selected.PageIndex));
        global::Windows.Foundation.Point corner;
        try
        {
            corner = slot.Content.TransformToVisual(QuickActionsLayer).TransformPoint(new global::Windows.Foundation.Point(placed.Left, placed.Top));
        }
        catch (ArgumentException)
        {
            // The page is not in the live tree (yet): nothing to stand beside.
            return null;
        }
        QuickActionsBar.Visibility = Visibility.Visible;
        QuickActionsBar.Measure(new global::Windows.Foundation.Size(double.PositiveInfinity, double.PositiveInfinity));
        var size = QuickActionsBar.DesiredSize;
        return ContentQuickActions.BarOrigin(placed with { Left = corner.X, Top = corner.Y }, size.Width, size.Height,
            QuickActionsLayer.ActualWidth, QuickActionsLayer.ActualHeight);
    }
}
