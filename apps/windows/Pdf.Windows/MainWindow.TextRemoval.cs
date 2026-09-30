using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private bool _deletingText;

    private async void DeleteTextButton_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null || _organizing || _isBusy || _deletingText) return;
        _deletingText = true;
        var sessionId = _session.SessionId;
        try
        {
            await SettleContentEditorForHistoryAsync();
            if (_session?.SessionId != sessionId) return;
            var pageIndex = (uint)_firstVisiblePage;
            var targets = await _facade.PageTextDeletionTargetsAsync(sessionId, pageIndex);
            if (_session?.SessionId != sessionId) return;
            if (!targets.IsSuccess)
            {
                AnnotationStatus.Text = targets.Error!.Message;
                return;
            }
            if (targets.Value!.Count == 0)
            {
                AnnotationStatus.Text = "The visible page has no editable text runs.";
                return;
            }
            var choice = new ComboBox { Header = "Text run", HorizontalAlignment = HorizontalAlignment.Stretch };
            foreach (var run in targets.Value)
                choice.Items.Add($"{choice.Items.Count + 1}: {run.Text}");
            var detail = new TextBlock { TextWrapping = TextWrapping.Wrap };
            choice.SelectionChanged += (_, _) =>
            {
                if (choice.SelectedIndex < 0) return;
                var run = targets.Value[choice.SelectedIndex];
                detail.Text = $"X {run.Bounds.X:0.##}, Y {run.Bounds.Y:0.##} pt\n{run.Text}";
            };
            choice.SelectedIndex = 0;
            var panel = new StackPanel { Spacing = 8 };
            panel.Children.Add(choice);
            panel.Children.Add(detail);
            panel.Children.Add(new TextBlock
            {
                Text = "Delete the selected text run from this page. You can undo this change. Save first if this run already has a pending edit. This is not secure redaction.",
                TextWrapping = TextWrapping.Wrap,
            });
            var dialog = new ContentDialog
            {
                XamlRoot = PageScroller.XamlRoot,
                Title = $"Delete text — page {pageIndex + 1}",
                Content = panel,
                PrimaryButtonText = "Delete",
                CloseButtonText = "Cancel",
                DefaultButton = ContentDialogButton.Close,
            };
            if (await ShowModalAsync(dialog) != ContentDialogResult.Primary || _session?.SessionId != sessionId) return;
            var result = await _facade.RemoveTextRunAsync(sessionId, targets.Value[choice.SelectedIndex]);
            if (_session?.SessionId != sessionId) return;
            // The deletion remains undoable if rebuilding its preview failed.
            _contentEditedPages.Add(pageIndex);
            CancelContentEditor();
            _pageContent.Remove(pageIndex);
            foreach (var key in _pendingRunBounds.Keys.Where(key => key.Page == pageIndex).ToArray())
                _pendingRunBounds.Remove(key);
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
            AnnotationStatus.Text = "Text deleted. Save to keep the change.";
        }
        finally { _deletingText = false; }
    }
}
