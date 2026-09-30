using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private async void InsertTextButton_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null || _organizing || _isBusy || _editingTextGeometry) return;
        _editingTextGeometry = true;
        var sessionId = _session.SessionId;
        try
        {
            await SettleContentEditorForHistoryAsync();
            if (_session?.SessionId != sessionId) return;
            var pageIndex = (uint)_firstVisiblePage;
            var prepared = await _facade.PrepareTextInsertionAsync(sessionId, pageIndex);
            if (_session?.SessionId != sessionId) return;
            if (!prepared.IsSuccess) { AnnotationStatus.Text = prepared.Error!.Message; return; }
            var text = new TextBox { Header = "Text", AcceptsReturn = false };
            var x = new NumberBox { Header = "X (pt)", Value = 36 };
            var y = new NumberBox { Header = "Y (pt)", Value = 36 };
            var size = new NumberBox { Header = "Size (pt)", Minimum = 1, Maximum = 72, Value = 14 };
            var panel = new StackPanel { Spacing = 8 };
            panel.Children.Add(text);
            panel.Children.Add(x);
            panel.Children.Add(y);
            panel.Children.Add(size);
            panel.Children.Add(new TextBlock
            {
                Text = "Adds a single line of Helvetica text as page content. X increases rightward and Y upward from the bottom-left of the unrotated page. Some characters cannot be encoded in this font.",
                TextWrapping = TextWrapping.Wrap,
            });
            var dialog = new ContentDialog
            {
                XamlRoot = PageScroller.XamlRoot, Title = $"Insert text — page {pageIndex + 1}", Content = panel,
                PrimaryButtonText = "Insert", CloseButtonText = "Cancel", DefaultButton = ContentDialogButton.Close,
            };
            void validate() => dialog.IsPrimaryButtonEnabled = !string.IsNullOrWhiteSpace(text.Text)
                && text.Text.IndexOfAny(['\r', '\n']) < 0 && double.IsFinite(x.Value) && double.IsFinite(y.Value)
                && double.IsFinite(size.Value) && size.Value >= 1 && size.Value <= 72;
            text.TextChanged += (_, _) => validate();
            x.ValueChanged += (_, _) => validate();
            y.ValueChanged += (_, _) => validate();
            size.ValueChanged += (_, _) => validate();
            validate();
            if (await ShowModalAsync(dialog) != ContentDialogResult.Primary || _session?.SessionId != sessionId) return;
            var result = await _facade.InsertTextRunAsync(sessionId, prepared.Value!, text.Text, x.Value, y.Value, size.Value);
            if (_session?.SessionId != sessionId) return;
            // A recorded edit remains undoable even if its preview refresh failed.
            _contentEditedPages.Add(pageIndex);
            CancelContentEditor();
            _pageContent.Remove(pageIndex);
            if (pageIndex < _slots.Count) ClearContentOutlines(_slots[(int)pageIndex]);
            InvalidatePageCharacters(pageIndex);
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
            AnnotationStatus.Text = "Text inserted. Save to keep the change.";
        }
        finally { _editingTextGeometry = false; }
    }
}
