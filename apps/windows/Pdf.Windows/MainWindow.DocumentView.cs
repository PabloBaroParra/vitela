using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Pdf.Windows.Localization;
using Windows.Security.Cryptography;
using Windows.Storage;

namespace Pdf.Windows;

/// <summary>Installs a successfully opened document — or a tab's parked one — into the existing view state.</summary>
public sealed partial class MainWindow
{
    /// <summary>
    /// A picked, dropped or recent file: one user action, so it lands in front
    /// in a tab of its own — or brings forward the tab already holding it.
    /// See <c>MainWindow.TabOpen.cs</c>.
    /// </summary>
    private Task OpenStorageFileAsync(StorageFile file) => OpenFilesInTabsAsync([file]);

    /// <summary>Reads a file's bytes for an open, reporting a failure the way an open does.</summary>
    private async Task<byte[]?> ReadStorageFileAsync(StorageFile file)
    {
        try
        {
            var buffer = await FileIO.ReadBufferAsync(file);
            CryptographicBuffer.CopyToByteArray(buffer, out byte[] bytes);
            return bytes;
        }
        catch (Exception error)
        {
            ReportFailedOpen(_facade.OpenReadFailure(error).Error!);
            return null;
        }
    }

    /// <param name="origin">The file the document came from, which Save suggests; <c>null</c> when it has none.</param>
    private void ShowOpenedDocument(DocumentSession session, string? origin = null)
    {
        // Whatever replaced the previous document — a save-and-reopen, or an
        // open after its changes were saved or discarded — left its recovery
        // entry with nothing to protect.
        ForgetRecoveryEntry();
        InstallDocumentView(session, origin);
    }

    /// <summary>
    /// Puts <paramref name="session"/> on screen with every per-document piece
    /// of view state reset — a fresh open, or a parked tab coming back. Unlike
    /// <see cref="ShowOpenedDocument"/> it keeps the document's recovery entry:
    /// a tab switch replaces nothing.
    /// </summary>
    private void InstallDocumentView(DocumentSession session, string? origin)
    {
        _documentOrigin = origin;
        _stampPreviews.BeginSession(session.SessionId);
        // The cards describe the document being replaced.
        CloseOrganizeView();
        _pagesEdited = false;
        _viewerStale = false;
        _armedAnnotation = null;
        SyncAnnotationToolButtons();
        _selectedAnnotationId = null;
        _annotationState = null;
        _pointerDrag = null;
        ResetSelectionState();
        // The parsed page content and any open editor belong to the document
        // being replaced — its run ids mean nothing against the new bytes.
        ResetContentEditMode();
        ResetFormFieldState();
        _session = session;
        ShowEditorView();
        RefreshSessionCommands();
        _ = RefreshDocumentInfoAsync();
        _ = RefreshFormFieldsAsync();
        _ = RefreshSigningStateAsync();
        DocumentTitle.Text = _session.DisplayName;
        ActiveTabSessionInstalled();
        ClearSearchResults();
        // The status line describes the previous document's last action.
        AnnotationStatus.Text = "";
        if (_session.State == DocumentSessionState.Empty)
        {
            ShowEmpty(I18n.T("open-no-pages"));
            AnnotationStatus.Text = I18n.T("open-no-pages");
            _ = RefreshAnnotationStateAsync();
            return;
        }

        EmptyState.Visibility = Visibility.Collapsed;
        ErrorState.Visibility = Visibility.Collapsed;
        ShowDocumentPages(_session);
        _ = RefreshAnnotationStateAsync();
    }
}
