using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private bool _resizingImage;

    private async void ResizeImageButton_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null || _organizing || _isBusy || _resizingImage) return;
        _resizingImage = true;
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
            var width = new NumberBox { Header = "Width (pt)" };
            var height = new NumberBox { Header = "Height (pt)" };
            var panel = new StackPanel { Spacing = 8 };
            panel.Children.Add(choice);
            panel.Children.Add(width);
            panel.Children.Add(height);
            panel.Children.Add(new TextBlock { Text = "The image stays at its current origin. Width and height are independent.", TextWrapping = TextWrapping.Wrap });
            var dialog = new ContentDialog
            {
                XamlRoot = PageScroller.XamlRoot,
                Title = $"Resize image — page {pageIndex + 1}",
                Content = panel,
                PrimaryButtonText = "Resize",
                CloseButtonText = "Cancel",
                DefaultButton = ContentDialogButton.Primary,
            };
            void validate() => dialog.IsPrimaryButtonEnabled =
                choice.SelectedIndex >= 0 && double.IsFinite(width.Value) && width.Value > 0 && double.IsFinite(height.Value) && height.Value > 0;
            choice.SelectionChanged += (_, _) =>
            {
                if (choice.SelectedIndex < 0)
                {
                    validate();
                    return;
                }
                var image = images.Value[choice.SelectedIndex];
                width.Value = image.Bounds.Width;
                height.Value = image.Bounds.Height;
                validate();
            };
            width.ValueChanged += (_, _) => validate();
            height.ValueChanged += (_, _) => validate();
            choice.SelectedIndex = 0;
            if (await dialog.ShowAsync() != ContentDialogResult.Primary || _session?.SessionId != sessionId) return;

            var selected = images.Value[choice.SelectedIndex];
            var result = await _facade.ResizeImageAsync(sessionId, selected, width.Value, height.Value);
            if (_session?.SessionId != sessionId) return;
            if (result.IsSuccess && selected.Bounds.Width == width.Value && selected.Bounds.Height == height.Value)
            {
                AnnotationStatus.Text = "Image dimensions unchanged.";
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
            _annotationState = result.Value;
            UpdateAnnotationControls(_annotationState);
            AnnotationStatus.Text = "Image resized. Save to keep the change.";
        }
        finally
        {
            _resizingImage = false;
        }
    }
}
