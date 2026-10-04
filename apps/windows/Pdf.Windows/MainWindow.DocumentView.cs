using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Windows.Security.Cryptography;
using Windows.Storage;

namespace Pdf.Windows;

/// <summary>Installs a successfully opened document into the existing view state.</summary>
public sealed partial class MainWindow
{
    /// <summary>All picked/dropped/recent files share read errors, password retry and the unsaved-edits guard.</summary>
    private async Task OpenStorageFileAsync(StorageFile file)
    {
        SetBusy(true);
        byte[] bytes;
        try
        {
            var buffer = await FileIO.ReadBufferAsync(file);
            CryptographicBuffer.CopyToByteArray(buffer, out bytes);
        }
        catch (Exception error)
        {
            SetBusy(false);
            ReportFailedOpen(_facade.OpenReadFailure(error).Error!);
            return;
        }

        var previous = _session;
        await OpenDocumentAsync(file.Name, bytes);
        if (_session is not null && !ReferenceEquals(previous, _session)) WindowsRecentDocuments.Remember(file.Path);
    }

    private void ShowOpenedDocument(DocumentSession session)
    {
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
        if (HomeView.Visibility == Visibility.Visible) MarkRailDestination("Annotate");
        ShowEditorView();
        RefreshSessionCommands();
        _ = RefreshDocumentInfoAsync();
        _ = RefreshFormFieldsAsync();
        _ = RefreshSigningStateAsync();
        DocumentTitle.Text = _session.DisplayName;
        ClearSearchResults();
        // The status line describes the previous document's last action.
        AnnotationStatus.Text = "";
        if (_session.State == DocumentSessionState.Empty)
        {
            ShowEmpty("The PDF contains no pages.");
            AnnotationStatus.Text = "The PDF contains no pages.";
            _ = RefreshAnnotationStateAsync();
            return;
        }

        EmptyState.Visibility = Visibility.Collapsed;
        ErrorState.Visibility = Visibility.Collapsed;
        ShowDocumentPages(_session);
        _ = RefreshAnnotationStateAsync();
    }
}
