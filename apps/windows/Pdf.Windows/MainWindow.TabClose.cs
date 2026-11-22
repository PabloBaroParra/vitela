using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Facade;
using Pdf.Windows.Localization;
using Pdf.Windows.Recovery;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Closing tabs (GTK <c>tabs/close.rs</c>): one at a time from the strip,
/// Ctrl+W or Ctrl+F4, and every dirty one in turn when the window closes.
/// Unsaved work is asked about with the shared Save/Discard/Cancel prompt
/// (<c>MainWindow.LifecycleDialogs.cs</c>); closing the last tab returns to Home.
/// </summary>
public sealed partial class MainWindow
{
    private async void CloseTab_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        if (_tabList.Active is not { } id)
        {
            AnnotationStatus.Text = I18n.T("tabs-no-document");
            return;
        }
        await RequestCloseTabAsync(id);
    }

    /// <summary>A close button or Ctrl+W: closes the tab, saying why when it cannot.</summary>
    private async Task RequestCloseTabAsync(TabId id)
    {
        if (!_tabs.TryGetValue(id, out var tab))
        {
            AnnotationStatus.Text = I18n.T("tabs-gone");
            return;
        }
        if (TabSwitchRefusal() is { } refusal)
        {
            AnnotationStatus.Text = refusal;
            return;
        }
        _tabSwitching = true;
        try
        {
            if (_tabList.Active != id)
            {
                // A clean background tab just goes. A dirty one comes forward
                // first, so the reader sees what the question is about.
                if (!await TabIsDirtyAsync(tab))
                {
                    DropTab(id);
                    return;
                }
                if (!await SwitchToTabAsync(id, toEditor: false)) return;
            }
            if (!await ResolveActiveUnsavedAsync(closingWindow: false)) return;
            await RemoveActiveTabAsync();
        }
        finally
        {
            _tabSwitching = false;
        }
    }

    /// <summary>
    /// Whether a tab's document has unsaved changes. A question the facade
    /// cannot answer counts as yes: only a deliberate choice loses work.
    /// </summary>
    private async Task<bool> TabIsDirtyAsync(DocumentTab tab)
    {
        // The tab's own record, never _session: while a switch is installing a
        // tab, _session still names the document leaving the screen.
        if (tab.Session?.SessionId is not { } sessionId) return false;
        var pending = await tab.Facade.HasUnsavedChangesAsync(sessionId);
        return !pending.IsSuccess || pending.Value;
    }

    /// <summary>
    /// Settles the active tab's inline edit and, when it has unsaved changes,
    /// asks Save/Discard/Cancel. <c>true</c> when it may go: it was clean, the
    /// reader discarded, or the save succeeded.
    /// </summary>
    private async Task<bool> ResolveActiveUnsavedAsync(bool closingWindow)
    {
        string? sessionId;
        OperationResult<bool>? pending;
        SetBusy(true);
        try
        {
            if (!await PrepareDocumentLifecycleAsync()) return false;
            sessionId = _session?.SessionId;
            pending = sessionId is null ? null : await _facade.HasUnsavedChangesAsync(sessionId);
        }
        finally
        {
            SetBusy(false);
            RestoreAnnotationControls();
        }
        if (pending is { IsSuccess: false })
        {
            AnnotationStatus.Text = pending.Error!.Message;
            return false;
        }
        if (pending?.Value == true)
        {
            var decision = await AskPendingEditDecisionAsync(
                closing: closingWindow,
                detail: closingWindow ? null : I18n.T("tabs-unsaved-detail-closing"));
            if (decision == PendingEditDecision.Cancel) return false;
            if (decision == PendingEditDecision.Save && !await SaveToPickedFileAsync()) return false;
        }
        return _session?.SessionId == sessionId;
    }

    /// <summary>
    /// Every tab with unsaved changes, in turn, before the window closes — the
    /// active one first, then each other dirty one brought forward. One Cancel
    /// keeps the window and every document open; a Discard loses nothing until
    /// the window actually closes.
    /// </summary>
    private async Task<bool> ResolveUnsavedTabsForCloseAsync()
    {
        if (!await ResolveActiveUnsavedAsync(closingWindow: true)) return false;
        var resolved = _tabList.Active;
        foreach (var id in _tabList.Ids)
        {
            if (id == resolved || !_tabs.TryGetValue(id, out var tab) || !await TabIsDirtyAsync(tab)) continue;
            if (!await SwitchToTabAsync(id, toEditor: false)) return false;
            if (!await ResolveActiveUnsavedAsync(closingWindow: true)) return false;
        }
        return true;
    }

    /// <summary>
    /// Closes the active tab, whose unsaved work was already resolved: the tab
    /// that takes over comes on screen, or Home when it was the last one.
    /// </summary>
    private async Task RemoveActiveTabAsync()
    {
        if (_tabList.Active is not { } id || !_tabs.TryGetValue(id, out var closing)) return;
        var name = _session?.DisplayName ?? closing.Session?.DisplayName ?? "";
        var after = _tabList.Close(id);
        _tabs.Remove(id);
        ShutDownTabAutosave(closing);
        if (after is { Kind: AfterCloseKind.Activate, Next: { } next } && _tabs.TryGetValue(next, out var successor))
        {
            await InstallTabAsync(next, successor);
        }
        else
        {
            _facade = NewDocumentFacade();
            ShowNoDocument();
        }
        // Only once nothing on screen reads it any more.
        closing.Facade.Dispose();
        AnnotationStatus.Text = I18n.T("tabs-closed", ("name", name));
        SyncTabStrip();
    }

    /// <summary>Closes a clean background tab: nothing on screen changes.</summary>
    private void DropTab(TabId id)
    {
        if (!_tabs.TryGetValue(id, out var tab)) return;
        _tabList.Close(id);
        _tabs.Remove(id);
        ShutDownTabAutosave(tab);
        tab.Facade.Dispose();
        AnnotationStatus.Text = I18n.T("tabs-closed", ("name", tab.Session?.DisplayName ?? ""));
        SyncTabStrip();
    }

    /// <summary>A closed tab's document was saved or discarded: its recovery entry protects nothing.</summary>
    private static void ShutDownTabAutosave(DocumentTab tab) => tab.Autosave?.Shutdown(AutosaveScheduler.ShutdownWait);
}
