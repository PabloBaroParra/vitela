using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Localization;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Switching document tabs (GTK <c>tabs/park.rs</c>): a strip click or
/// Ctrl+Tab parks the tab on screen and brings another back, rebuilt from its
/// still-open facade at the reading position it was left at. The last tab
/// closing leaves the window with no document, on Home.
/// </summary>
public sealed partial class MainWindow
{
    /// <summary>A click on a tab: switches to it, saying on the status line why when it cannot.</summary>
    private async Task RequestTabActivationAsync(TabId id)
    {
        if (_tabList.Active == id)
        {
            if (HomeView.Visibility == Visibility.Visible) ShowEditorView();
            return;
        }
        if (TabSwitchRefusal() is { } refusal)
        {
            AnnotationStatus.Text = refusal;
            return;
        }
        await SwitchToTabAsync(id, toEditor: false);
    }

    private async void NextTab_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args) =>
        await CycleTabAsync(+1, args);

    private async void PreviousTab_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args) =>
        await CycleTabAsync(-1, args);

    /// <summary>Ctrl+Tab and Ctrl+Shift+Tab; with fewer than two tabs the key is left to the focused control.</summary>
    private async Task CycleTabAsync(int step, KeyboardAcceleratorInvokedEventArgs args)
    {
        if (_tabList.Cycle(step) is not { } next)
        {
            args.Handled = false;
            return;
        }
        args.Handled = true;
        await RequestTabActivationAsync(next);
    }

    /// <summary>
    /// Brings tab <paramref name="id"/> on screen. No refusal check: callers
    /// either made it (<see cref="RequestTabActivationAsync"/>) or are a flow
    /// that owns the window already (an open, a close).
    /// </summary>
    /// <param name="toEditor">An open of a file this tab holds lands on the editor, as a new tab would; a click keeps the screen.</param>
    /// <returns>Whether the tab is now active.</returns>
    private async Task<bool> SwitchToTabAsync(TabId id, bool toEditor)
    {
        if (!_tabs.TryGetValue(id, out var target))
        {
            AnnotationStatus.Text = I18n.T("tabs-gone");
            return false;
        }
        if (_tabList.Active == id)
        {
            if (toEditor) ShowEditorView();
            return true;
        }
        var wasSwitching = _tabSwitching;
        _tabSwitching = true;
        try
        {
            var onHome = HomeView.Visibility == Visibility.Visible;
            if (!await ParkActiveTabAsync()) return false;
            await InstallTabAsync(id, target);
            if (onHome && !toEditor) ShowHomeView();
            return true;
        }
        finally
        {
            _tabSwitching = wasSwitching;
        }
    }

    /// <summary>
    /// Settles the active tab's inline edit and parks it. <c>false</c>, with
    /// the reason on the status line, when an inline edit the core refused is
    /// still open: leaving would strand it.
    /// </summary>
    private async Task<bool> ParkActiveTabAsync()
    {
        if (ActiveTab is null) return true;
        if (!await SettleActiveTabAsync()) return false;
        ParkActiveTab();
        return true;
    }

    /// <summary>Commits the active tab's open inline edit, as every lifecycle step does first.</summary>
    private async Task<bool> SettleActiveTabAsync()
    {
        if (ActiveTab is null) return true;
        SetBusy(true);
        bool settled;
        try
        {
            settled = await PrepareDocumentLifecycleAsync();
        }
        finally
        {
            SetBusy(false);
            RestoreAnnotationControls();
        }
        if (!settled) AnnotationStatus.Text = I18n.T("tabs-finish-editing");
        return settled;
    }

    /// <summary>Records what the active tab needs to come back, and leaves no tab active.</summary>
    private void ParkActiveTab()
    {
        if (ActiveTab is not { } tab) return;
        tab.Session = _session;
        tab.Origin = _documentOrigin;
        tab.Anchor = _session is { PageCount: > 0 } && _spans.Count > 0 && PageScroller.Visibility == Visibility.Visible
            ? CaptureAnchor()
            : null;
        tab.Organizing = _organizing;
        tab.PagesEdited = _pagesEdited;
        tab.ContentEditedPages = [.. _contentEditedPages];
        tab.FilledFieldPages = [.. _filledFieldPages];
        _tabList.Deactivate();
    }

    /// <summary>
    /// Puts a parked tab's document on screen: its facade becomes the window's,
    /// its session is read again (Organize or undo may have moved pages since
    /// it was last seen), and it comes back at its reading position.
    /// </summary>
    private async Task InstallTabAsync(TabId id, DocumentTab tab)
    {
        _facade = tab.Facade;
        _tabList.Activate(id);
        var session = tab.Session;
        if (session is not null)
        {
            SetBusy(true);
            var fresh = await _facade.SessionAsync(session.SessionId);
            SetBusy(false);
            if (fresh.IsSuccess) session = fresh.Value!;
        }
        if (session is null)
        {
            ShowNoDocument();
            return;
        }
        InstallDocumentView(session, tab.Origin);
        // The view is rebuilt from the fresh session, so it is not stale; what
        // this session has edited still is, for every later undo or redo.
        _pagesEdited = tab.PagesEdited;
        _contentEditedPages.UnionWith(tab.ContentEditedPages);
        _filledFieldPages.UnionWith(tab.FilledFieldPages);
        RestoreAnnotationControls();
        RestoreReadingPosition(tab.Anchor);
        if (tab.Organizing) await EnterOrganizeViewAsync();
    }

    private void RestoreReadingPosition(ScrollAnchor? anchor)
    {
        if (anchor is not { } position || _spans.Count == 0 || PageScroller.Visibility != Visibility.Visible) return;
        PageScroller.UpdateLayout();
        var span = _spans[Math.Clamp(position.PageIndex, 0, _spans.Count - 1)];
        PageScroller.ChangeView(null, span.Top + (position.Fraction * span.Height), null, disableAnimation: true);
        UpdateViewport(intermediate: false);
    }

    /// <summary>
    /// The window with no document: what it was at start-up. Every piece of
    /// per-document view state goes, as <see cref="InstallDocumentView"/>
    /// resets it for a new one, and the reader lands on Home.
    /// </summary>
    private void ShowNoDocument()
    {
        _documentOrigin = null;
        CloseOrganizeView();
        _pagesEdited = false;
        _viewerStale = false;
        _armedAnnotation = null;
        SyncAnnotationToolButtons();
        _selectedAnnotationId = null;
        _annotationState = null;
        _pointerDrag = null;
        ResetSelectionState();
        ResetContentEditMode();
        ResetFormFieldState();
        _session = null;
        PageStack.Children.Clear();
        _slots = [];
        _spans = [];
        ClearSearchResults();
        DocumentTitle.Text = I18n.T("editor-no-document");
        EmptyStateMessage.Text = I18n.T("editor-empty-state");
        EmptyState.Visibility = Visibility.Visible;
        ErrorState.Visibility = Visibility.Collapsed;
        PageScroller.Visibility = Visibility.Collapsed;
        PageCounter.Text = "";
        PageNavigationList.ItemsSource = null;
        ZoomLevel.Text = "";
        RefreshSessionCommands();
        UpdateAnnotationControls(null);
        _ = RefreshDocumentInfoAsync();
        _ = RefreshSigningStateAsync();
        ShowHomeView();
        SyncTabStrip();
    }
}
