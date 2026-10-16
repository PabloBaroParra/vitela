using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Add watermark: stamping the same text across the pages the reader picks,
/// from the Edit PDF page's "Pages" card.
/// </summary>
/// <remarks>
/// <para>
/// A dialog rather than an armed canvas click like Insert text, for the
/// reason the GTK shell gives: a watermark has no position to pick — the core
/// centres it on every page it names — and the one thing it does need is a
/// page list, which no single click can give.
/// </para>
/// <para>
/// The stamp is recorded from inside the dialog's primary button, so a
/// refusal — blank text, a bad range, a character Helvetica cannot show —
/// is shown under the fields and the reader corrects it in place, the way
/// Export images handles a bad range.
/// </para>
/// </remarks>
public sealed partial class MainWindow
{
    private bool _addingWatermark;

    private async void AddWatermarkButton_Click(object sender, RoutedEventArgs e)
    {
        if (_addingWatermark || _isBusy || _dialogOpen || _organizing || _session is not { PageCount: > 0 } session) return;
        if (!session.ContentEditingAllowed)
        {
            AnnotationStatus.Text = "This document does not permit content changes.";
            return;
        }
        _addingWatermark = true;
        try
        {
            // A retype still open in the editor is committed first, so the
            // stamp lands after it in the log and one undo takes only the stamp.
            await SettleContentEditorForHistoryAsync();
            if (_session?.SessionId != session.SessionId) return;
            var added = await AskWatermarkAsync(session);
            if (_session?.SessionId != session.SessionId) return;
            if (added is null)
            {
                AnnotationStatus.Text = "Watermark cancelled.";
                return;
            }
            await AfterWatermarkAsync(added);
        }
        finally
        {
            _addingWatermark = false;
        }
    }

    /// <summary>
    /// What a recorded stamp made stale: the pages it covers are re-rendered
    /// now and remembered for undo, which re-renders every content-edited page.
    /// </summary>
    private async Task AfterWatermarkAsync(WatermarkAdded added)
    {
        foreach (var pageIndex in added.Pages)
        {
            _contentEditedPages.Add(pageIndex);
            _pageContent.Remove(pageIndex);
            InvalidatePageCharacters(pageIndex);
            if (pageIndex < _slots.Count)
            {
                ClearContentOutlines(_slots[(int)pageIndex]);
                _slots[(int)pageIndex].Render.DropBitmap();
                _slots[(int)pageIndex].Tiles.Clear();
            }
        }
        // Once, not per page: a stamp on every page of a long document would
        // otherwise recompute the viewport once for each of them.
        UpdateViewport(intermediate: false);
        RedrawAnnotations();

        if (!added.Preview.IsSuccess)
        {
            // Recorded and undoable all the same; only the preview is behind.
            await RefreshAnnotationStateAsync();
            AnnotationStatus.Text = added.Preview.Error!.Message;
            return;
        }
        _annotationState = added.Preview.Value;
        UpdateAnnotationControls(_annotationState);
        AnnotationStatus.Text = WatermarkWording.Summary(added.Pages.Count);
    }

    /// <summary>
    /// Text, pages, size, opacity and angle in one dialog, starting from the
    /// core's default diagonal look. Returns the recorded stamp, or
    /// <c>null</c> when the reader cancelled.
    /// </summary>
    private async Task<WatermarkAdded?> AskWatermarkAsync(DocumentSession session)
    {
        var currentPage = Math.Min((uint)Math.Max(0, _firstVisiblePage), session.PageCount - 1);
        var look = _facade.DefaultWatermarkLook();

        var text = new TextBox { Header = "Text", Text = WatermarkLimits.DefaultText, MinWidth = 260, Name = "WatermarkText" };
        text.Loaded += (_, _) =>
        {
            // Selected, so typing replaces the placeholder word.
            text.SelectAll();
            text.Focus(FocusState.Programmatic);
        };

        var pages = new RadioButtons { Header = "Pages" };
        var range = new TextBox { PlaceholderText = "1-3,7", MinWidth = 200, IsEnabled = false, Name = "WatermarkPageRange" };
        AutomationProperties.SetName(range, "Page range");
        pages.Items.Add(new RadioButton { Content = "All pages", Tag = WatermarkPages.All });
        pages.Items.Add(new RadioButton { Content = "Current page", Tag = WatermarkPages.Current });
        var custom = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        custom.Children.Add(new TextBlock { Text = "Pages", VerticalAlignment = VerticalAlignment.Center });
        custom.Children.Add(range);
        var customChoice = new RadioButton { Content = custom, Tag = WatermarkPages.Custom };
        AutomationProperties.SetName(customChoice, "Pages");
        pages.Items.Add(customChoice);
        pages.SelectedIndex = 0;
        pages.SelectionChanged += (_, _) =>
        {
            range.IsEnabled = SelectedWatermarkPages(pages) == WatermarkPages.Custom;
            if (range.IsEnabled) range.Focus(FocusState.Programmatic);
        };

        var size = LookBox("Size (pt)", WatermarkLimits.MinFontSizePt, WatermarkLimits.MaxFontSizePt, look.FontSizePt, 10);
        var opacity = LookBox("Opacity (%)", WatermarkLimits.MinOpacityPercent, WatermarkLimits.MaxOpacityPercent, look.OpacityPercent, 10);
        var angle = LookBox("Angle (°)", WatermarkLimits.MinRotationDegrees, WatermarkLimits.MaxRotationDegrees, look.RotationDegrees, 15);
        var lookRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 12 };
        lookRow.Children.Add(size);
        lookRow.Children.Add(opacity);
        lookRow.Children.Add(angle);

        var refusal = new TextBlock { Name = "WatermarkValidationError", TextWrapping = TextWrapping.Wrap, Visibility = Visibility.Collapsed };
        refusal.Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["SystemFillColorCriticalBrush"];

        var panel = new StackPanel { Spacing = 12, MaxWidth = 440 };
        panel.Children.Add(text);
        panel.Children.Add(pages);
        panel.Children.Add(lookRow);
        panel.Children.Add(new TextBlock
        {
            Text = "A watermark is painted into the pages themselves and is one undo step.",
            TextWrapping = TextWrapping.Wrap,
            Style = NamedStyle("EditHintStyle"),
        });
        panel.Children.Add(refusal);

        var dialog = new ContentDialog
        {
            Title = "Add watermark",
            Content = panel,
            PrimaryButtonText = "Add",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };

        WatermarkAdded? added = null;
        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            try
            {
                var request = new WatermarkRequest(
                    SelectedWatermarkPages(pages),
                    range.Text,
                    currentPage,
                    text.Text,
                    // A cleared NumberBox reports NaN; the core's look stands in.
                    double.IsNaN(size.Value) ? look.FontSizePt : size.Value,
                    double.IsNaN(opacity.Value) ? look.OpacityPercent : opacity.Value,
                    double.IsNaN(angle.Value) ? look.RotationDegrees : angle.Value);
                var result = await _facade.AddWatermarkAsync(session.SessionId, request);
                if (result.IsSuccess)
                {
                    added = result.Value;
                    return;
                }

                refusal.Text = result.Error!.Message;
                refusal.Visibility = Visibility.Visible;
                args.Cancel = true;
            }
            finally
            {
                deferral.Complete();
            }
        };

        // Recorded is recorded: even a dialog that somehow did not report
        // Primary must not lose a stamp the core already holds.
        await ShowModalAsync(dialog);
        return added;
    }

    private static NumberBox LookBox(string header, double minimum, double maximum, double value, double largeChange) => new()
    {
        Header = header,
        Minimum = minimum,
        Maximum = maximum,
        Value = Math.Clamp(value, minimum, maximum),
        SmallChange = 1,
        LargeChange = largeChange,
        SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Compact,
        ValidationMode = NumberBoxValidationMode.InvalidInputOverwritten,
    };

    private static WatermarkPages SelectedWatermarkPages(RadioButtons pages) =>
        (pages.SelectedItem as RadioButton)?.Tag is WatermarkPages chosen ? chosen : WatermarkPages.All;
}
