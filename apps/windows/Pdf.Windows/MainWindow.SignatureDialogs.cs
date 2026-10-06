using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>The two dialogs of Draw signature: the pad, and the offer of the remembered signature.</summary>
public sealed partial class MainWindow
{
    private sealed record DrawnPad(IReadOnlyList<IReadOnlyList<PadPoint>> Strokes, double StrokeWidth, bool Remember);

    private enum SavedSignatureChoice { Cancel, Use, DrawNew, Delete }

    /// <summary>
    /// The pad. <b>Use</b> stays disabled until a line exists, <b>Clear</b>
    /// wipes it without closing. <b>Remember on this PC</b> is ticked by default.
    /// Null when cancelled.
    /// </summary>
    private async Task<DrawnPad?> AskDrawnSignatureAsync()
    {
        var pad = new SignaturePadView();
        var remember = new CheckBox { Content = "Remember on this PC", IsChecked = true };
        AutomationProperties.SetAutomationId(remember, "RememberSignatureCheckBox");
        var content = new StackPanel { Spacing = 8, MinWidth = 360 };
        content.Children.Add(pad);
        content.Children.Add(remember);
        var dialog = new ContentDialog
        {
            XamlRoot = Content.XamlRoot, Title = "Draw your signature", Content = content,
            PrimaryButtonText = "Use", SecondaryButtonText = "Clear", CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close, IsPrimaryButtonEnabled = false, IsSecondaryButtonEnabled = false,
        };
        pad.InkChanged += (_, _) =>
        {
            dialog.IsPrimaryButtonEnabled = pad.HasInk;
            dialog.IsSecondaryButtonEnabled = pad.HasContent;
        };
        dialog.SecondaryButtonClick += (_, args) =>
        {
            args.Cancel = true; // Clear keeps the dialog open
            pad.Clear();
        };
        if (await ShowModalAsync(dialog) != ContentDialogResult.Primary || !pad.HasInk) return null;
        return new DrawnPad(pad.Strokes, SignaturePadView.StrokeWidth, remember.IsChecked == true);
    }

    /// <summary>
    /// The signature remembered on this PC, offered before a blank pad: <b>Use</b>
    /// places it as it is, <b>Draw new</b> opens the pad, <b>Delete</b> forgets it
    /// here (no confirmation). Shown on white, as the pad is — the ink is black.
    /// </summary>
    private async Task<SavedSignatureChoice> AskSavedSignatureAsync(byte[] png)
    {
        BitmapImage? picture = null;
        try { picture = await DecodeStampPreviewAsync(png); }
        catch (Exception) { /* a file that no longer decodes can still be deleted or replaced */ }

        var frame = new Border
        {
            Background = new SolidColorBrush(Colors.White), Height = 160, Padding = new Thickness(12),
            BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(4),
            BorderBrush = (Brush)Application.Current.Resources["ControlStrokeColorDefaultBrush"],
            Child = picture is null
                ? new TextBlock { Text = "This saved signature can no longer be shown.", Foreground = new SolidColorBrush(Colors.Black), TextWrapping = TextWrapping.Wrap }
                : new Image { Source = picture, Stretch = Stretch.Uniform },
        };
        AutomationProperties.SetName(frame, "Your saved signature");
        AutomationProperties.SetAutomationId(frame, "SavedSignature");
        var delete = new Button { Content = "Delete" };
        AutomationProperties.SetAutomationId(delete, "DeleteSavedSignature");
        var content = new StackPanel { Spacing = 8, MinWidth = 360 };
        content.Children.Add(frame);
        content.Children.Add(delete);
        var dialog = new ContentDialog
        {
            XamlRoot = Content.XamlRoot, Title = "Your signature", Content = content,
            PrimaryButtonText = "Use", SecondaryButtonText = "Draw new", CloseButtonText = "Cancel",
            DefaultButton = picture is null ? ContentDialogButton.Close : ContentDialogButton.Primary,
            IsPrimaryButtonEnabled = picture is not null,
        };
        var deleted = false;
        delete.Click += (_, _) =>
        {
            deleted = true;
            dialog.Hide();
        };
        var result = await ShowModalAsync(dialog);
        if (deleted) return SavedSignatureChoice.Delete;
        return result switch
        {
            ContentDialogResult.Primary => SavedSignatureChoice.Use,
            ContentDialogResult.Secondary => SavedSignatureChoice.DrawNew,
            _ => SavedSignatureChoice.Cancel,
        };
    }
}
