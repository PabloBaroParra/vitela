using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>
/// Doc-wide exact-text search: running the query, listing the hits, and
/// painting the matching PDF-space character geometry over the page.
/// </summary>
public sealed partial class MainWindow
{
    private async void SearchButton_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null)
        {
            return;
        }

        var query = SearchBox.Text.Trim();
        ClearSearchResults();
        if (query.Length == 0)
        {
            SearchStatus.Text = "Enter text to find.";
            return;
        }

        SearchButton.IsEnabled = false;
        var result = await _facade.SearchAsync(_session.SessionId, query);
        SearchButton.IsEnabled = true;
        if (result.IsDiscarded)
        {
            return;
        }

        if (!result.IsSuccess)
        {
            SearchStatus.Text = result.Error!.Message;
            return;
        }

        var search = result.Value!;
        SearchStatus.Text = search.Hits.Count == 0 ? "No matches." : $"{search.Hits.Count} match(es).";
        foreach (var hit in search.Hits)
        {
            SearchResultsList.Items.Add(new ListViewItem
            {
                Content = $"Page {hit.PageIndex + 1}: {hit.Text}",
                Tag = hit,
            });
        }
    }

    private async void SearchResultsList_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_session is null || SearchResultsList.SelectedItem is not ListViewItem { Tag: SearchHit hit })
        {
            return;
        }

        var navigation = await _facade.NavigateToSearchResultAsync(_session.SessionId, hit);
        if (!navigation.IsSuccess)
        {
            ShowError(navigation.Error!);
            return;
        }

        _session = navigation.Value!;
        var pageIndex = checked((int)hit.PageIndex);
        PageScroller.ChangeView(null, _spans[pageIndex].Top, null, disableAnimation: false);
        ShowSearchHighlight(pageIndex, hit.CharacterBounds);
    }

    private void ClearSearchResults()
    {
        SearchResultsList.Items.Clear();
        SearchStatus.Text = "";
        foreach (var slot in _slots)
        {
            slot.Highlights.Children.Clear();
        }
    }

    private void ShowSearchHighlight(int pageIndex, IReadOnlyList<SearchRect> bounds)
    {
        foreach (var slot in _slots)
        {
            slot.Highlights.Children.Clear();
        }

        var page = _session!.Pages[pageIndex];
        var target = _slots[pageIndex];
        // Highlights follow the page they sit on, so they track the current
        // zoom instead of a scale fixed at build time.
        var scale = target.Scale;
        foreach (var boundsRect in bounds)
        {
            var rectangle = new Rectangle
            {
                Width = Math.Max(1, boundsRect.WidthPt * scale),
                Height = Math.Max(1, boundsRect.HeightPt * scale),
                Fill = new SolidColorBrush(global::Windows.UI.Color.FromArgb(96, 255, 214, 10)),
                Stroke = new SolidColorBrush(Microsoft.UI.Colors.DarkOrange),
                StrokeThickness = 1,
            };
            Canvas.SetLeft(rectangle, boundsRect.XPt * scale);
            Canvas.SetTop(rectangle, (page.HeightPt - boundsRect.YPt - boundsRect.HeightPt) * scale);
            target.Highlights.Children.Add(rectangle);
        }
    }
}
