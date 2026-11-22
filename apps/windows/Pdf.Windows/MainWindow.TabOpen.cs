using Pdf.Windows.Facade;
using Pdf.Windows.Localization;
using Pdf.Windows.Viewer;
using Windows.Storage;

namespace Pdf.Windows;

/// <summary>
/// Opens into document tabs (GTK <c>tabs/open.rs</c>): every open lands in a
/// tab of its own, so nothing on screen is replaced and there is no unsaved
/// work to ask about. The first file of a user action lands in front when the
/// active tab can give way; the rest land in background tabs. A file some tab
/// already holds is never opened twice: its tab is brought forward, or — when
/// it is the tab on screen — read again, after Save/Discard/Cancel if it has
/// unsaved changes. Placement is decided by <see cref="TabOpenRouting"/>.
/// </summary>
public sealed partial class MainWindow
{
    /// <summary>Whether a batch of opens is running; a second one is refused rather than interleaved.</summary>
    private bool _tabsOpening;

    /// <summary>
    /// One file of an open. <see cref="OpenAsync"/> opens it into the facade it
    /// is given — a new tab's, or the active tab's for a reload — and answers
    /// <c>null</c> when it was abandoned (a cancelled password, an unreadable
    /// file already reported).
    /// </summary>
    /// <param name="Path">The file it is read from, or <c>null</c> for a sample or a blank document.</param>
    private sealed record TabOpenRequest(
        string DisplayName,
        string? Path,
        Func<PdfDocumentFacade, bool, Task<OperationResult<DocumentSession>?>> OpenAsync);

    private Task OpenFilesInTabsAsync(IReadOnlyList<StorageFile> files) =>
        OpenInTabsAsync(files.Select(file => new TabOpenRequest(file.Name, file.Path,
            (facade, discard) => OpenBytesAsync(facade, file.Name, () => ReadStorageFileAsync(file), discard))).ToList());

    private Task OpenBytesInTabAsync(string displayName, byte[] bytes, string? path = null) =>
        OpenInTabsAsync([new TabOpenRequest(displayName, path,
            (facade, discard) => OpenBytesAsync(facade, displayName, () => Task.FromResult<byte[]?>(bytes), discard))]);

    /// <summary>Command-line files waiting for the shell to finish building; see <see cref="OpenAtStartup"/>.</summary>
    private IReadOnlyList<string>? _startupPaths;

    /// <summary>Opens <paramref name="paths"/> as tabs now, or once the shell is built.</summary>
    internal void OpenAtStartup(IReadOnlyList<string> paths)
    {
        if (_shellInitialized) _ = OpenPathsAsync(paths);
        else _startupPaths = paths;
    }

    /// <summary>Called once the shell is built: opens what the command line named.</summary>
    private void OpenStartupPaths()
    {
        if (_startupPaths is not { Count: > 0 } paths) return;
        _startupPaths = null;
        _ = OpenPathsAsync(paths);
    }

    /// <summary>
    /// Opens command-line files — Explorer's "Open with" on several PDFs, or a
    /// shortcut's arguments — as one user action. Missing files are skipped.
    /// </summary>
    internal async Task OpenPathsAsync(IReadOnlyList<string> paths)
    {
        var files = new List<StorageFile>();
        foreach (var path in paths)
        {
            try
            {
                files.Add(await StorageFile.GetFileFromPathAsync(Path.GetFullPath(path)));
            }
            catch (Exception error)
            {
                AnnotationStatus.Text = _facade.OpenReadFailure(error).Error!.Message;
            }
        }
        if (files.Count > 0) await OpenFilesInTabsAsync(files);
    }

    /// <summary>Reads and opens one file's bytes into <paramref name="facade"/>, asking for a password when it needs one.</summary>
    private async Task<OperationResult<DocumentSession>?> OpenBytesAsync(
        PdfDocumentFacade facade, string displayName, Func<Task<byte[]?>> read, bool discardPendingEdits)
    {
        SetBusy(true);
        byte[]? bytes;
        OperationResult<DocumentSession> result;
        try
        {
            bytes = await read();
            if (bytes is null) return null;
            result = await facade.OpenAsync(new DocumentSource(displayName, bytes), discardPendingEdits: discardPendingEdits);
        }
        finally
        {
            SetBusy(false);
        }
        if (!result.IsSuccess && result.Error!.RequiresPassword)
        {
            return await OpenWithPasswordAsync(displayName, bytes, discardPendingEdits, facade);
        }
        return result;
    }

    /// <summary>The files of one user action, in order; the first one may land in front.</summary>
    private async Task OpenInTabsAsync(IReadOnlyList<TabOpenRequest> requests)
    {
        if (requests.Count == 0 || _tabsOpening) return;
        _tabsOpening = true;
        try
        {
            for (var index = 0; index < requests.Count; index++)
            {
                if (_windowClosed) return;
                await OpenOneInTabAsync(requests[index], TabOpenRouting.IsFront(index, _tabList.Active is not null));
            }
        }
        finally
        {
            _tabsOpening = false;
            if (!_windowClosed)
            {
                RestoreAnnotationControls();
                SyncTabStrip();
            }
        }
    }

    private async Task OpenOneInTabAsync(TabOpenRequest request, bool front)
    {
        var holder = _tabList.FindByPath(request.Path);
        // Only an open that would take the screen settles the active tab's
        // inline edit; one that cannot be settled sends the open to the back.
        var frontAllowed = front && await SettleActiveTabAsync();
        var route = TabOpenRouting.Route(front, frontAllowed, holder, _tabList.Active);
        switch (route)
        {
            case TabOpenRoute.Skip:
                return;
            case TabOpenRoute.SwitchTo:
                await SwitchToTabAsync(holder!.Value, toEditor: true);
                return;
            case TabOpenRoute.Reload:
                await ReloadActiveTabAsync(request);
                return;
        }

        var facade = NewDocumentFacade();
        OperationResult<DocumentSession>? result;
        try
        {
            result = await request.OpenAsync(facade, false);
        }
        catch
        {
            facade.Dispose();
            throw;
        }
        if (result is not { IsSuccess: true })
        {
            facade.Dispose();
            if (result is { } failed) ReportFailedOpen(failed.Error!);
            else RestoreAnnotationControls();
            return;
        }
        if (route == TabOpenRoute.OpenInFront) LandInFront(facade, result.Value!, request.Path);
        else LandInBackground(facade, result.Value!, request.Path);
        RememberRecent(request.Path);
    }

    /// <summary>
    /// A new tab, shown at once. The tab that was on screen — already settled
    /// by the caller — is parked first.
    /// </summary>
    private void LandInFront(PdfDocumentFacade facade, DocumentSession session, string? origin)
    {
        // With no tab yet, the window's facade holds nothing: the new one replaces it.
        if (ActiveTab is null && _session is null) _facade.Dispose();
        ParkActiveTab();
        var id = _tabList.Add(origin);
        _tabs[id] = NewTab(facade);
        _tabList.Activate(id);
        _facade = facade;
        ShowOpenedDocument(session, origin);
        RestoreAnnotationControls();
    }

    /// <summary>A new tab behind the one on screen, which stays exactly as it was.</summary>
    private void LandInBackground(PdfDocumentFacade facade, DocumentSession session, string? origin)
    {
        var id = _tabList.Add(origin);
        var tab = NewTab(facade);
        tab.Session = session;
        tab.Origin = origin;
        _tabs[id] = tab;
        AnnotationStatus.Text = I18n.T("tabs-opened-in-background", ("name", session.DisplayName));
        SyncTabStrip();
    }

    /// <summary>
    /// The file on screen, opened again: read it from disk once more, into its
    /// own tab. Unsaved changes are asked about first, and the file is read only
    /// after the answer, so a Save onto this very file is what gets read.
    /// </summary>
    private async Task ReloadActiveTabAsync(TabOpenRequest request)
    {
        ShowEditorView();
        var sessionId = _session?.SessionId;
        var discard = false;
        if (sessionId is not null)
        {
            SetBusy(true);
            var pending = await _facade.HasUnsavedChangesAsync(sessionId);
            SetBusy(false);
            RestoreAnnotationControls();
            if (!pending.IsSuccess)
            {
                AnnotationStatus.Text = pending.Error!.Message;
                return;
            }
            if (pending.Value)
            {
                switch (await AskPendingEditDecisionAsync(detail: I18n.T("tabs-unsaved-detail-reloading")))
                {
                    case PendingEditDecision.Cancel:
                        return;
                    case PendingEditDecision.Save when !await SaveToPickedFileAsync():
                        return;
                    case PendingEditDecision.Discard:
                        discard = true;
                        break;
                }
            }
        }
        // A Save As elsewhere moved this tab off the file, or the document changed under the question.
        if (_session?.SessionId != sessionId || _tabList.FindByPath(request.Path) != _tabList.Active)
        {
            AnnotationStatus.Text = I18n.T("tabs-reload-skipped");
            return;
        }

        var result = await request.OpenAsync(_facade, discard);
        if (result is null)
        {
            RestoreAnnotationControls();
            return;
        }
        if (!result.IsSuccess)
        {
            ReportFailedOpen(result.Error!);
            return;
        }
        ShowOpenedDocument(result.Value!, request.Path);
        RestoreAnnotationControls();
        RememberRecent(request.Path);
    }

    private void RememberRecent(string? path)
    {
        if (TabPaths.Normalize(path) is not null) WindowsRecentDocuments.Remember(path!, _hiddenRecents);
    }
}
