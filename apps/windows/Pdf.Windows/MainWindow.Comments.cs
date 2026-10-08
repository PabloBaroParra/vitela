using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>Read-only comment presentation; core owns the merged snapshot.</summary>
public sealed partial class MainWindow
{
    private readonly StackPanel _commentsList = new() { Spacing = 8 };
    private IReadOnlyList<Comment>? _shownComments;

    private void BuildCommentsPanel()
    {
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetAutomationId(_commentsList, "CommentsList");
        _toolPages["Comments"].Children.Add(_commentsList);
        UpdateCommentsPanel(null);
    }

    private void UpdateCommentsPanel(AnnotationState? state)
    {
        var comments = state?.Comments ?? [];
        if (!ReferenceEquals(comments, _shownComments))
        {
            _shownComments = comments;
            _commentsList.Children.Clear();
            if (comments.Count == 0)
                _commentsList.Children.Add(new TextBlock { Text = "No comments in this document.", TextWrapping = TextWrapping.Wrap });
            foreach (var page in comments.GroupBy(comment => comment.PageIndex).OrderBy(group => group.Key))
            {
                _commentsList.Children.Add(new TextBlock { Text = $"Page {page.Key + 1}", FontWeight = Microsoft.UI.Text.FontWeights.SemiBold });
                foreach (var comment in page)
                {
                    var content = new StackPanel { Spacing = 4 };
                    content.Children.Add(new TextBlock { Text = comment.Contents, TextWrapping = TextWrapping.Wrap, MaxLines = 3 });
                    var metadata = string.Join(" · ", new[] { comment.Author, comment.Date }.Where(value => !string.IsNullOrEmpty(value)));
                    if (metadata.Length > 0) content.Children.Add(new TextBlock { Text = metadata, TextWrapping = TextWrapping.Wrap });
                    var button = new Button { Content = content, Tag = comment, HorizontalAlignment = HorizontalAlignment.Stretch, HorizontalContentAlignment = HorizontalAlignment.Stretch };
                    var sessionId = state?.SessionId;
                    button.Click += async (_, _) => { if (_session?.SessionId == sessionId) await ReadCommentAsync(comment); };
                    _commentsList.Children.Add(button);
                }
            }
        }
        foreach (var button in _commentsList.Children.OfType<Button>())
            button.IsEnabled = _session is not null && !_isBusy && !_organizing && !_dialogOpen;
    }

    private async Task ReadCommentAsync(Comment comment)
    {
        if (_session is null || _isBusy || _organizing || _dialogOpen || comment.PageIndex >= _slots.Count) return;
        _selectedAnnotationId = comment.AnnotationId;
        UpdateAnnotationControls(_annotationState);
        RedrawAnnotations();
        var index = (int)comment.PageIndex;
        var slot = _slots[index];
        var placed = _facade.PlaceRect(comment.Rect, PlacementOf(slot, index));
        slot.Annotations.StartBringIntoView(new BringIntoViewOptions
        {
            TargetRect = new global::Windows.Foundation.Rect(placed.Left, placed.Top, Math.Max(1, placed.Width), Math.Max(1, placed.Height)),
            AnimationDesired = false,
        });
        await ShowModalAsync(new ContentDialog
        {
            XamlRoot = PageScroller.XamlRoot,
            Title = $"Comment — page {comment.PageIndex + 1}",
            Content = new TextBox { Header = string.Join(" · ", new[] { comment.Author, comment.Date }.Where(value => !string.IsNullOrEmpty(value))),
                IsReadOnly = true, AcceptsReturn = true, Text = comment.Contents, TextWrapping = TextWrapping.Wrap, MinWidth = 300, MinHeight = 120, MaxHeight = 300 },
            CloseButtonText = "Close",
            DefaultButton = ContentDialogButton.Close,
        });
    }
}
