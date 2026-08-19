using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
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
    /// <summary>
    /// Ctrl+F moves focus to the search box rather than running a search —
    /// there is nothing to search for yet, and this is the same "get me to
    /// the query field" behaviour the GTK shell's <c>win.find</c> action
    /// gives Ctrl+F.
    /// </summary>
    private void FindDocument_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        SearchBox.Focus(FocusState.Programmatic);
    }

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

    /// <summary>
    /// The hit currently painted, kept so a zoom can repaint it. The geometry
    /// is in PDF points and the paint multiplies by <c>slot.Scale</c>, so a
    /// highlight drawn once and left alone would keep the scale it was drawn
    /// at — the same defect the annotation overlay had.
    /// </summary>
    private (int PageIndex, IReadOnlyList<SearchRect> Bounds)? _searchHighlight;

    private void ClearSearchResults()
    {
        SearchResultsList.Items.Clear();
        SearchStatus.Text = "";
        _searchHighlight = null;
        foreach (var slot in _slots)
        {
            slot.SearchHighlights.Children.Clear();
        }
    }

    private void ShowSearchHighlight(int pageIndex, IReadOnlyList<SearchRect> bounds)
    {
        _searchHighlight = (pageIndex, bounds);
        RedrawSearchHighlight();
    }

    private void RedrawSearchHighlight()
    {
        foreach (var slot in _slots)
        {
            slot.SearchHighlights.Children.Clear();
        }

        if (_searchHighlight is not (var pageIndex, var bounds)) return;
        if (_session is null || pageIndex >= _slots.Count || pageIndex >= _session.Pages.Count) return;

        var page = _session.Pages[pageIndex];
        var target = _slots[pageIndex];
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
            target.SearchHighlights.Children.Add(rectangle);
        }
    }
}
