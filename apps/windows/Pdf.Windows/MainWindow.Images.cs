using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private enum ImageAction { Resize, Move }
    private bool _editingImage;

    private async void ResizeImageButton_Click(object sender, RoutedEventArgs e) => await EditImageAsync(ImageAction.Resize);

    private async void MoveImageButton_Click(object sender, RoutedEventArgs e) => await EditImageAsync(ImageAction.Move);

    private async Task EditImageAsync(ImageAction action)
    {
        if (_session is null || _organizing || _isBusy || _editingImage) return;
        _editingImage = true;
        var move = action == ImageAction.Move;
        var sessionId = _session.SessionId;
        try
        {
            await SettleContentEditorForHistoryAsync();
            if (_session?.SessionId != sessionId) return;
            var pageIndex = (uint)_firstVisiblePage;
            var images = await _facade.PageImagesAsync(sessionId, pageIndex);
            if (_session?.SessionId != sessionId) return;
            if (!images.IsSuccess)
            {
                AnnotationStatus.Text = images.Error!.Message;
                return;
            }
            if (images.Value!.Count == 0)
            {
                AnnotationStatus.Text = "The visible page has no editable content images.";
                return;
            }

            var choice = new ComboBox { Header = "Image", HorizontalAlignment = HorizontalAlignment.Stretch };
            foreach (var image in images.Value)
                choice.Items.Add($"Image {choice.Items.Count + 1}: X {image.Bounds.X:0.##}, Y {image.Bounds.Y:0.##} pt");
            var first = new NumberBox { Header = move ? "X (pt)" : "Width (pt)" };
            var second = new NumberBox { Header = move ? "Y (pt)" : "Height (pt)" };
            var panel = new StackPanel { Spacing = 8 };
            panel.Children.Add(choice);
            panel.Children.Add(first);
            panel.Children.Add(second);
            panel.Children.Add(new TextBlock
            {
                Text = (move ? "Coordinates use PDF space: X increases rightward and Y upward. The image keeps its size."
                    : "The image stays at its current origin. Width and height are independent.")
                    + " Save before making another geometry edit to the same image.",
                TextWrapping = TextWrapping.Wrap,
            });
            var dialog = new ContentDialog
            {
                XamlRoot = PageScroller.XamlRoot,
                Title = $"{(move ? "Move" : "Resize")} image — page {pageIndex + 1}",
                Content = panel,
                PrimaryButtonText = move ? "Move" : "Resize",
                CloseButtonText = "Cancel",
                DefaultButton = ContentDialogButton.Primary,
            };
            void validate() => dialog.IsPrimaryButtonEnabled =
                choice.SelectedIndex >= 0 && ((double.IsFinite(first.Value) && double.IsFinite(second.Value)
                && (move || (first.Value > 0 && second.Value > 0))));
            choice.SelectionChanged += (_, _) =>
            {
                if (choice.SelectedIndex < 0)
                {
                    validate();
                    return;
                }
                var image = images.Value[choice.SelectedIndex];
                first.Value = move ? image.Bounds.X : image.Bounds.Width;
                second.Value = move ? image.Bounds.Y : image.Bounds.Height;
                validate();
            };
            first.ValueChanged += (_, _) => validate();
            second.ValueChanged += (_, _) => validate();
            choice.SelectedIndex = 0;
            if (await dialog.ShowAsync() != ContentDialogResult.Primary || _session?.SessionId != sessionId) return;

            var selected = images.Value[choice.SelectedIndex];
            var result = move
                ? await _facade.MoveImageAsync(sessionId, selected, first.Value, second.Value)
                : await _facade.ResizeImageAsync(sessionId, selected, first.Value, second.Value);
            if (_session?.SessionId != sessionId) return;
            var unchanged = move
                ? selected.Bounds.X == first.Value && selected.Bounds.Y == second.Value
                : selected.Bounds.Width == first.Value && selected.Bounds.Height == second.Value;
            if (result.IsSuccess && unchanged)
            {
                AnnotationStatus.Text = move ? "Image position unchanged." : "Image dimensions unchanged.";
                return;
            }
            // An edit remains undoable even if rebuilding its preview failed.
            _contentEditedPages.Add(pageIndex);
            CancelContentEditor();
            _pageContent.Remove(pageIndex);
            foreach (var key in _pendingRunBounds.Keys.Where(key => key.Page == pageIndex).ToArray())
                _pendingRunBounds.Remove(key);
            InvalidatePageRender(pageIndex);
            if (!result.IsSuccess)
            {
                await RefreshAnnotationStateAsync();
                AnnotationStatus.Text = result.Error!.Message;
                return;
            }
            _pendingImageEdits.Add((selected.PageIndex, selected.Id));
            _annotationState = result.Value;
            UpdateAnnotationControls(_annotationState);
            AnnotationStatus.Text = move ? "Image moved. Save to keep the change." : "Image resized. Save to keep the change.";
        }
        finally
        {
            _editingImage = false;
        }
    }
}
