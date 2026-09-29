using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.System;

namespace Pdf.Windows;

/// <summary>
/// Doc-wide exact-text search: running the query, listing the hits, and
/// painting the matching PDF-space character geometry over the page.
/// </summary>
public sealed partial class MainWindow
{
    private uint _searchGeneration;
    private string? _searchQuery;
    private readonly Dictionary<uint, List<SearchHit>> _searchHitsByPage = [];
    private SearchHit? _paintedHit;

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

    private async void SearchBox_KeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != VirtualKey.Enter) return;
        e.Handled = true;
        if (SearchButton.IsEnabled) await RunSearchAsync();
    }

    private async void SearchButton_Click(object sender, RoutedEventArgs e) => await RunSearchAsync();

    private async Task RunSearchAsync()
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

        var sessionId = _session.SessionId;
        var generation = _searchGeneration;
        SearchButton.IsEnabled = false;
        SearchStatus.Text = $"Searching for \"{query}\"...";
        var result = await _facade.SearchAsync(sessionId, query);
        if (generation != _searchGeneration || _session?.SessionId != sessionId || result.IsDiscarded)
        {
            return;
        }
        SearchButton.IsEnabled = !_isBusy;

        if (!result.IsSuccess)
        {
            SearchStatus.Text = result.Error!.Message;
            return;
        }

        var search = result.Value!;
        SearchStatus.Text = search.Hits.Count == 0 ? $"No matches for \"{query}\"." : $"{search.Hits.Count} match(es).";
        _searchQuery = search.Hits.Count == 0 ? null : query;
        foreach (var hit in search.Hits)
        {
            if (!_searchHitsByPage.TryGetValue(hit.PageIndex, out var pageHits))
            {
                pageHits = [];
                _searchHitsByPage.Add(hit.PageIndex, pageHits);
            }
            pageHits.Add(hit);
            SearchResultsList.Items.Add(new ListViewItem
            {
                Content = $"Page {hit.PageIndex + 1}: {hit.Text}",
                Tag = hit,
            });
        }
        RedrawSearchHighlights();
        UpdateMatchButtons();
        if (search.Hits.Count > 0) SearchResultsList.SelectedIndex = 0;
    }

    private void PreviousMatchButton_Click(object sender, RoutedEventArgs e) => StepMatch(-1);

    private void NextMatchButton_Click(object sender, RoutedEventArgs e) => StepMatch(1);

    private void StepMatch(int delta)
    {
        if (_session is null || _isBusy) return;
        SearchResultsList.SelectedIndex = SearchSelection.StepIndex(SearchResultsList.SelectedIndex, SearchResultsList.Items.Count, delta);
    }

    private void UpdateMatchButtons()
    {
        var enabled = !_isBusy && _session is not null && SearchResultsList.Items.Count > 0;
        PreviousMatchButton.IsEnabled = enabled;
        NextMatchButton.IsEnabled = enabled;
    }

    private async void SearchResultsList_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_session is null || SearchResultsList.SelectedItem is not ListViewItem { Tag: SearchHit hit } selected)
        {
            return;
        }

        var sessionId = _session.SessionId;
        var generation = _searchGeneration;
        var navigation = await _facade.NavigateToSearchResultAsync(sessionId, hit);
        if (_session?.SessionId != sessionId || generation != _searchGeneration || !ReferenceEquals(SearchResultsList.SelectedItem, selected)) return;
        if (!navigation.IsSuccess)
        {
            ShowError(navigation.Error!);
            return;
        }

        _session = navigation.Value!;
        var pageIndex = checked((int)hit.PageIndex);
        PageScroller.ChangeView(null, _spans[pageIndex].Top, null, disableAnimation: false);
        var previousPage = _paintedHit?.PageIndex;
        _paintedHit = hit;
        if (previousPage is { } oldPage && oldPage != hit.PageIndex) RedrawSearchHighlights((int)oldPage);
        RedrawSearchHighlights(pageIndex);
        if (_searchQuery is not null)
        {
            SearchStatus.Text = SearchSelection.Status(_searchQuery, SearchResultsList.SelectedIndex, SearchResultsList.Items.Count);
        }
    }

    private void ClearSearchResults()
    {
        _searchGeneration++;
        _searchQuery = null;
        _paintedHit = null;
        _searchHitsByPage.Clear();
        SearchResultsList.Items.Clear();
        UpdateMatchButtons();
        SearchStatus.Text = "";
        foreach (var slot in _slots)
        {
            slot.SearchHighlights.Children.Clear();
        }
    }

    /// <summary>Repaint PDF-space hits at the current zoom; only the pages whose accent changed need repainting on navigation.</summary>
    private void RedrawSearchHighlights(int? pageIndex = null)
    {
        var start = pageIndex ?? 0;
        var end = pageIndex is null ? _slots.Count : start + 1;
        for (var index = start; index < end; index++)
        {
            var target = _slots[index];
            target.SearchHighlights.Children.Clear();
            if (_session is null || index >= _session.Pages.Count || !_searchHitsByPage.TryGetValue((uint)index, out var hits)) continue;

            // Paint the selected match last so it remains distinct even when
            // two results have overlapping character bounds.
            foreach (var hit in hits)
            {
                if (!ReferenceEquals(hit, _paintedHit)) PaintSearchHit(target, index, hit, selected: false);
            }
            if (_paintedHit is { } selected && selected.PageIndex == (uint)index)
            {
                PaintSearchHit(target, index, selected, selected: true);
            }
        }
    }

    private void PaintSearchHit(PageSlot target, int pageIndex, SearchHit hit, bool selected)
    {
        foreach (var boundsRect in hit.CharacterBounds)
        {
            var rectangle = new Rectangle
            {
                Fill = new SolidColorBrush(selected
                    ? global::Windows.UI.Color.FromArgb(153, 242, 140, 26)
                    : global::Windows.UI.Color.FromArgb(102, 242, 204, 51)),
            };
            PlaceOverPage(rectangle, target, pageIndex, new AnnotationRect(boundsRect.XPt, boundsRect.YPt, boundsRect.WidthPt, boundsRect.HeightPt));
            target.SearchHighlights.Children.Add(rectangle);
        }
    }
}
