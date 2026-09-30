using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Security.Cryptography;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private async void ReplaceImageButton_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null || _organizing || _isBusy || _editingImage) return;
        _editingImage = true;
        var sessionId = _session.SessionId;
        try
        {
            await SettleContentEditorForHistoryAsync();
            if (_session?.SessionId != sessionId) return;
            var pageIndex = (uint)_firstVisiblePage;
            var images = await _facade.PageImagesAsync(sessionId, pageIndex);
            if (_session?.SessionId != sessionId) return;
            if (!images.IsSuccess) { AnnotationStatus.Text = images.Error!.Message; return; }
            if (images.Value!.Count == 0) { AnnotationStatus.Text = "The visible page has no editable content images."; return; }
            var choice = new ComboBox { Header = "Image", HorizontalAlignment = HorizontalAlignment.Stretch };
            foreach (var image in images.Value)
                choice.Items.Add($"Image {choice.Items.Count + 1}: X {image.Bounds.X:0.##}, Y {image.Bounds.Y:0.##} pt");
            choice.SelectedIndex = 0;
            var panel = new StackPanel { Spacing = 8 };
            panel.Children.Add(choice);
            panel.Children.Add(new TextBlock
            {
                Text = "Choose a PNG or JPEG to replace this image. Its position and size stay fixed, so different proportions may stretch. Save first if this image already has a pending edit. Some original encodings cannot be recovered for undo and are refused.",
                TextWrapping = TextWrapping.Wrap,
            });
            var dialog = new ContentDialog
            {
                XamlRoot = PageScroller.XamlRoot, Title = $"Replace image — page {pageIndex + 1}", Content = panel,
                PrimaryButtonText = "Choose file", CloseButtonText = "Cancel", DefaultButton = ContentDialogButton.Close,
            };
            choice.SelectionChanged += (_, _) => dialog.IsPrimaryButtonEnabled = choice.SelectedIndex >= 0;
            if (await ShowModalAsync(dialog) != ContentDialogResult.Primary || _session?.SessionId != sessionId) return;
            var selected = images.Value[choice.SelectedIndex];
            var prepared = await _facade.PrepareImageReplacementAsync(sessionId, selected);
            if (_session?.SessionId != sessionId) return;
            if (!prepared.IsSuccess) { AnnotationStatus.Text = prepared.Error!.Message; return; }
            var picker = new FileOpenPicker();
            foreach (var extension in new[] { ".png", ".jpg", ".jpeg" }) picker.FileTypeFilter.Add(extension);
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
            var file = await picker.PickSingleFileAsync();
            if (file is null || _session?.SessionId != sessionId) return;
            byte[] bytes;
            try
            {
                var buffer = await FileIO.ReadBufferAsync(file);
                CryptographicBuffer.CopyToByteArray(buffer, out bytes);
            }
            catch (Exception)
            {
                if (_session?.SessionId == sessionId) AnnotationStatus.Text = "The selected image could not be read.";
                return;
            }
            if (_session?.SessionId != sessionId) return;
            var result = await _facade.ReplaceContentImageAsync(sessionId, selected, bytes);
            if (_session?.SessionId != sessionId) return;
            // A recorded edit remains undoable even when rebuilding its preview fails.
            _contentEditedPages.Add(pageIndex);
            CancelContentEditor();
            _pageContent.Remove(pageIndex);
            if (pageIndex < _slots.Count) ClearContentOutlines(_slots[(int)pageIndex]);
            InvalidatePageRender(pageIndex);
            RedrawAnnotations();
            if (!result.IsSuccess)
            {
                await RefreshAnnotationStateAsync();
                AnnotationStatus.Text = result.Error!.Message;
                return;
            }
            _annotationState = result.Value;
            UpdateAnnotationControls(_annotationState);
            AnnotationStatus.Text = "Image replaced. Save to keep the change.";
        }
        finally { _editingImage = false; }
    }
}
