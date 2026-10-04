using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private bool _editingTextGeometry;

    private async void DeleteTextButton_Click(object sender, RoutedEventArgs e) => await DeleteOpenContentTextAsync();

    private async void MoveTextButton_Click(object sender, RoutedEventArgs e) => await MoveTextGeometryAsync();

    private async Task MoveTextGeometryAsync()
    {
        if (_session is null || _organizing || _isBusy || _editingTextGeometry) return;
        _editingTextGeometry = true;
        var sessionId = _session.SessionId;
        try
        {
            await SettleContentEditorForHistoryAsync();
            if (_session?.SessionId != sessionId) return;
            var pageIndex = (uint)_firstVisiblePage;
            var targets = await _facade.PageTextEditTargetsAsync(sessionId, pageIndex);
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
            var x = new NumberBox { Header = "X (pt)" };
            var y = new NumberBox { Header = "Y (pt)" };
            choice.SelectionChanged += (_, _) =>
            {
                if (choice.SelectedIndex < 0) return;
                var run = targets.Value[choice.SelectedIndex];
                detail.Text = $"X {run.Bounds.X:0.##}, Y {run.Bounds.Y:0.##} pt\n{run.Text}";
                x.Value = run.Bounds.X;
                y.Value = run.Bounds.Y;
            };
            choice.SelectedIndex = 0;
            var panel = new StackPanel { Spacing = 8 };
            panel.Children.Add(choice);
            panel.Children.Add(detail);
            panel.Children.Add(x);
            panel.Children.Add(y);
            panel.Children.Add(new TextBlock
            {
                Text = "Coordinates use PDF space: X increases rightward and Y upward. The text keeps its font and size. Repeated moves share one Undo step; moving newly inserted text updates its insertion. Save and reopen first if existing text has been retyped.",
                TextWrapping = TextWrapping.Wrap,
            });
            var dialog = new ContentDialog
            {
                XamlRoot = PageScroller.XamlRoot,
                Title = $"Move text — page {pageIndex + 1}",
                Content = panel,
                PrimaryButtonText = "Move",
                CloseButtonText = "Cancel",
                DefaultButton = ContentDialogButton.Close,
            };
            void validate() => dialog.IsPrimaryButtonEnabled = choice.SelectedIndex >= 0
                && double.IsFinite(x.Value) && double.IsFinite(y.Value);
            x.ValueChanged += (_, _) => validate();
            y.ValueChanged += (_, _) => validate();
            choice.SelectionChanged += (_, _) => validate();
            validate();
            if (await ShowModalAsync(dialog) != ContentDialogResult.Primary || _session?.SessionId != sessionId) return;
            var selected = targets.Value[choice.SelectedIndex];
            var result = await _facade.MoveTextRunAsync(sessionId, selected, x.Value, y.Value);
            if (_session?.SessionId != sessionId) return;
            if (result.IsSuccess && selected.Bounds.X == x.Value && selected.Bounds.Y == y.Value)
            {
                AnnotationStatus.Text = "Text position unchanged.";
                return;
            }
            // The edit remains undoable if rebuilding its preview failed.
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
            AnnotationStatus.Text = "Text moved. Save to keep the change.";
        }
        finally { _editingTextGeometry = false; }
    }
}
