using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>
/// The Add watermark dialog: Text or Picture, the pages, and the look —
/// starting from the core's own for whichever kind is showing.
/// </summary>
/// <remarks>
/// Switching kind resets the angle and the layer to the core's defaults for
/// that kind (diagonal text over the content, an upright picture behind it),
/// the way the GTK shell does: a logo left at text's 45° reads as a mistake,
/// and a background picture painted over the text hides the page. The numbers
/// come from <c>DefaultWatermarkLook</c> / <c>DefaultPictureWatermarkLook</c>,
/// never from constants here.
/// </remarks>
public sealed partial class MainWindow
{
    /// <summary>
    /// Returns the recorded stamp, or <c>null</c> when the reader cancelled.
    /// </summary>
    private async Task<WatermarkAdded?> AskWatermarkAsync(DocumentSession session)
    {
        var currentPage = Math.Min((uint)Math.Max(0, _firstVisiblePage), session.PageCount - 1);
        var textLook = _facade.DefaultWatermarkLook();
        var pictureLook = _facade.DefaultPictureWatermarkLook();

        var kind = new RadioButtons { Header = "Watermark", MaxColumns = 2 };
        kind.Items.Add(new RadioButton { Content = "Text", Tag = WatermarkKind.Text });
        kind.Items.Add(new RadioButton { Content = "Picture", Tag = WatermarkKind.Picture });
        kind.SelectedIndex = 0;

        var text = new TextBox { Header = "Text", Text = WatermarkLimits.DefaultText, MinWidth = 260, Name = "WatermarkText" };
        text.Loaded += (_, _) =>
        {
            // Selected, so typing replaces the placeholder word.
            text.SelectAll();
            text.Focus(FocusState.Programmatic);
        };
        var size = LookBox("Size (pt)", WatermarkLimits.MinFontSizePt, WatermarkLimits.MaxFontSizePt, textLook.FontSizePt, 10);
        var textPanel = new StackPanel { Spacing = 12 };
        textPanel.Children.Add(text);
        textPanel.Children.Add(size);

        // The picture's bytes are read once, when picked, and held here: the
        // core is handed them untouched when the stamp is recorded.
        byte[]? picture = null;
        var choose = new Button { Content = "Choose picture…", Name = "WatermarkChoosePicture" };
        var chosen = new TextBlock
        {
            Text = "No picture chosen. PNG or JPEG.",
            TextWrapping = TextWrapping.Wrap,
            Style = NamedStyle("EditHintStyle"),
        };
        choose.Click += async (_, _) =>
        {
            var bytes = await PickContentImageBytesAsync(session.SessionId);
            if (bytes is not { Length: > 0 }) return;
            picture = bytes;
            chosen.Text = $"Picture chosen ({Math.Max(1, bytes.Length / 1024)} KB).";
        };
        var scale = LookBox("Size (% of page)", WatermarkLimits.MinScalePercent, WatermarkLimits.MaxScalePercent, pictureLook.ScalePercent, 10);
        var picturePanel = new StackPanel { Spacing = 12, Visibility = Visibility.Collapsed };
        picturePanel.Children.Add(choose);
        picturePanel.Children.Add(chosen);
        picturePanel.Children.Add(scale);

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

        var opacity = LookBox("Opacity (%)", WatermarkLimits.MinOpacityPercent, WatermarkLimits.MaxOpacityPercent, textLook.OpacityPercent, 10);
        var angle = LookBox("Angle (°)", WatermarkLimits.MinRotationDegrees, WatermarkLimits.MaxRotationDegrees, textLook.RotationDegrees, 15);
        var lookRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 12 };
        lookRow.Children.Add(opacity);
        lookRow.Children.Add(angle);

        var layer = new RadioButtons { Header = "Placement", Name = "WatermarkLayer" };
        layer.Items.Add(new RadioButton { Content = "Over the page content", Tag = WatermarkLayer.OverContent });
        layer.Items.Add(new RadioButton { Content = "Behind the page content", Tag = WatermarkLayer.UnderContent });
        layer.SelectedIndex = LayerIndex(textLook.Layer);

        kind.SelectionChanged += (_, _) =>
        {
            var picturing = SelectedWatermarkKind(kind) == WatermarkKind.Picture;
            textPanel.Visibility = picturing ? Visibility.Collapsed : Visibility.Visible;
            picturePanel.Visibility = picturing ? Visibility.Visible : Visibility.Collapsed;
            angle.Value = picturing ? pictureLook.RotationDegrees : textLook.RotationDegrees;
            layer.SelectedIndex = LayerIndex(picturing ? pictureLook.Layer : textLook.Layer);
        };

        var refusal = new TextBlock { Name = "WatermarkValidationError", TextWrapping = TextWrapping.Wrap, Visibility = Visibility.Collapsed };
        refusal.Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["SystemFillColorCriticalBrush"];

        var panel = new StackPanel { Spacing = 12, MaxWidth = 440 };
        panel.Children.Add(kind);
        panel.Children.Add(textPanel);
        panel.Children.Add(picturePanel);
        panel.Children.Add(pages);
        panel.Children.Add(lookRow);
        panel.Children.Add(layer);
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
                // A cleared NumberBox reports NaN; the core's look stands in.
                WatermarkMark mark = SelectedWatermarkKind(kind) == WatermarkKind.Picture
                    ? new PictureMark(picture ?? [], double.IsNaN(scale.Value) ? pictureLook.ScalePercent : scale.Value)
                    : new TextMark(text.Text, double.IsNaN(size.Value) ? textLook.FontSizePt : size.Value);
                var request = new WatermarkRequest(
                    SelectedWatermarkPages(pages),
                    range.Text,
                    currentPage,
                    mark,
                    double.IsNaN(opacity.Value) ? textLook.OpacityPercent : opacity.Value,
                    double.IsNaN(angle.Value) ? textLook.RotationDegrees : angle.Value,
                    (layer.SelectedItem as RadioButton)?.Tag is WatermarkLayer chosenLayer ? chosenLayer : textLook.Layer);
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

    private enum WatermarkKind
    {
        Text,
        Picture,
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

    private static WatermarkKind SelectedWatermarkKind(RadioButtons kind) =>
        (kind.SelectedItem as RadioButton)?.Tag is WatermarkKind chosen ? chosen : WatermarkKind.Text;

    private static int LayerIndex(WatermarkLayer layer) => layer == WatermarkLayer.UnderContent ? 1 : 0;
}
