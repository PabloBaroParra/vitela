using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>Page-list navigation is viewport state, not an edit or a core render request.</summary>
public sealed partial class MainWindow
{
    private bool _syncingPageNavigation;
    private readonly RequestedPage _requestedPage = new();

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
        PageNavigationList.IsEnabled = !_isBusy && !_organizing && !_dialogOpen
            && _session is { PageCount: > 0 } && PageScroller.Visibility == Visibility.Visible;
        GoToPageButton.IsEnabled = PageNavigationList.IsEnabled;
        PreviousPageButton.IsEnabled = PageNavigationList.IsEnabled && _firstVisiblePage > 0;
        NextPageButton.IsEnabled = PageNavigationList.IsEnabled && _firstVisiblePage < _spans.Count - 1;
        FirstPageButton.IsEnabled = PreviousPageButton.IsEnabled;
        LastPageButton.IsEnabled = NextPageButton.IsEnabled;
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
        NavigateToPage(index);
    }

    private void NavigateToPage(int index)
    {
        if (index < 0 || index >= _spans.Count) return;
        // The spans are rebuilt at each zoom and page-structure change. Use
        // their current positions, not offsets captured when the list opened.
        var offset = _requestedPage.Request(index, _spans[index].Top, PageScroller.ScrollableHeight);
        PageScroller.ChangeView(null, offset, null, disableAnimation: true);
        UpdateViewport(intermediate: false);
    }

    private void PreviousPageButton_Click(object sender, RoutedEventArgs e) => StepPage(-1);

    private void NextPageButton_Click(object sender, RoutedEventArgs e) => StepPage(1);

    private void FirstPageButton_Click(object sender, RoutedEventArgs e) => JumpToPageBoundary(last: false);

    private void LastPageButton_Click(object sender, RoutedEventArgs e) => JumpToPageBoundary(last: true);

    private void StepPage(int delta) => NavigateFromToolbar(_firstVisiblePage + delta);

    private void JumpToPageBoundary(bool last) => NavigateFromToolbar(last ? _spans.Count - 1 : 0);

    private void NavigateFromToolbar(int index)
    {
        if (_session is not { PageCount: > 0 } || _isBusy || _organizing || _dialogOpen
            || PageScroller.Visibility != Visibility.Visible) return;
        NavigateToPage(index);
    }

    private async void GoToPageButton_Click(object sender, RoutedEventArgs e) => await GoToPageAsync();

    private async Task GoToPageAsync()
    {
        if (_session is not { PageCount: > 0 } session || _isBusy || _organizing || _dialogOpen
            || PageScroller.Visibility != Visibility.Visible) return;
        var pages = session.Pages;
        var number = new NumberBox { Header = $"Page number (1–{session.PageCount})", Value = _firstVisiblePage + 1 };
        var dialog = new ContentDialog
        {
            XamlRoot = PageScroller.XamlRoot,
            Title = "Go to page",
            Content = number,
            PrimaryButtonText = "Go",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
        };
        bool valid() => double.IsFinite(number.Value) && number.Value == Math.Truncate(number.Value)
            && number.Value >= 1 && number.Value <= session.PageCount;
        number.ValueChanged += (_, _) => dialog.IsPrimaryButtonEnabled = valid();
        dialog.IsPrimaryButtonEnabled = valid();
        dialog.Opened += (_, _) => number.Focus(FocusState.Programmatic);
        if (await ShowModalAsync(dialog) != ContentDialogResult.Primary) return;

        // A page number belongs to the captured document order, but its scroll
        // offset belongs to the current zoom. A refreshed page snapshot is
        // invalid even if every page has the same dimensions.
        if (_session?.SessionId != session.SessionId || !ReferenceEquals(_session.Pages, pages)
            || _isBusy || _organizing || PageScroller.Visibility != Visibility.Visible || !valid()) return;
        NavigateToPage((int)number.Value - 1);
    }
}
