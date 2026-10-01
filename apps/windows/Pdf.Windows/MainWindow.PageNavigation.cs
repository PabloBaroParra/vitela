using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Pdf.Windows;

/// <summary>Page-list navigation is viewport state, not an edit or a core render request.</summary>
public sealed partial class MainWindow
{
    private bool _syncingPageNavigation;

    private void RebuildPageNavigation()
    {
        _syncingPageNavigation = true;
        try
        {
            PageNavigationList.ItemsSource = Enumerable.Range(1, _slots.Count)
                .Select(number => $"Page {number}").ToArray();
        }
        finally { _syncingPageNavigation = false; }
        UpdatePageNavigationControls();
    }

    private void UpdatePageNavigationControls()
    {
        PageNavigationList.IsEnabled = !_isBusy && !_organizing
            && _session is { PageCount: > 0 } && PageScroller.Visibility == Visibility.Visible;
    }

    private void SyncPageNavigation()
    {
        UpdatePageNavigationControls();
        if (_organizing || _syncingPageNavigation || PageNavigationList.SelectedIndex == _firstVisiblePage) return;
        _syncingPageNavigation = true;
        try
        {
            PageNavigationList.SelectedIndex = _firstVisiblePage;
            if (PageNavigationList.SelectedItem is { } item)
                PageNavigationList.ScrollIntoView(item);
        }
        finally { _syncingPageNavigation = false; }
    }

    private void PageNavigationList_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_syncingPageNavigation || !PageNavigationList.IsEnabled) return;
        var index = PageNavigationList.SelectedIndex;
        if (index < 0 || index >= _spans.Count) return;
        // The spans are rebuilt at each zoom and page-structure change. Use
        // their current positions, not offsets captured when the list opened.
        PageScroller.ChangeView(null, _spans[index].Top, null, disableAnimation: true);
        UpdateViewport(intermediate: false);
    }
}
