using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Security.Cryptography;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private async void InsertImageButton_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null || _organizing || _isBusy || _editingImage) return;
        _editingImage = true;
        var sessionId = _session.SessionId;
        try
        {
            await SettleContentEditorForHistoryAsync();
            if (_session?.SessionId != sessionId) return;
            var pageIndex = (uint)_firstVisiblePage;
            var prepared = await _facade.PrepareImageInsertionAsync(sessionId, pageIndex);
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
            var measured = _facade.StampPlacement(bytes, 0, 0);
            if (!measured.IsSuccess) { AnnotationStatus.Text = measured.Error!.Message; return; }
            var x = new NumberBox { Header = "Top-left X (pt)", Value = 36 };
            var y = new NumberBox { Header = "Top-left Y (pt)", Value = 180 };
            var panel = new StackPanel { Spacing = 8 };
            panel.Children.Add(x);
            panel.Children.Add(y);
            panel.Children.Add(new TextBlock
            {
                Text = "Adds the image as page content, preserving its proportions with a longest side of 144 pt. X increases rightward and Y upward from the bottom-left of the unrotated page. Coordinates place the image's top-left corner.",
                TextWrapping = TextWrapping.Wrap,
            });
            var dialog = new ContentDialog
            {
                XamlRoot = PageScroller.XamlRoot, Title = $"Insert image — page {pageIndex + 1}", Content = panel,
                PrimaryButtonText = "Insert", CloseButtonText = "Cancel", DefaultButton = ContentDialogButton.Close,
            };
            void validate() => dialog.IsPrimaryButtonEnabled = double.IsFinite(x.Value) && double.IsFinite(y.Value);
            x.ValueChanged += (_, _) => validate();
            y.ValueChanged += (_, _) => validate();
            validate();
            if (await ShowModalAsync(dialog) != ContentDialogResult.Primary || _session?.SessionId != sessionId) return;
            var result = await _facade.InsertContentImageAsync(sessionId, prepared.Value!, bytes, x.Value, y.Value);
            if (_session?.SessionId != sessionId) return;
            // A recorded edit remains undoable even when refreshing its preview fails.
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
            AnnotationStatus.Text = "Image inserted. Save to keep the change.";
        }
        finally { _editingImage = false; }
    }
}
