using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// The Images card's state: which of Replace and Delete the selected canvas
/// image allows, the sentence that says why, and the two commands themselves.
/// </summary>
/// <remarks>
/// Mirrors the GTK shell's <c>content_edit::ImageControls</c>; the rule is
/// <see cref="ImageCardRules.Resolve"/>. The state is recomputed on every
/// update rather than cached, because the one temporary state (a pending
/// edit) clears on save and reopen, and a remembered answer would outlive it.
/// </remarks>
public sealed partial class MainWindow
{
    private const string NoImageSelectedHint = "Click an image on the page to select it.";
    private const string ImageSelectedHint = "Image selected — replace or delete it.";
    private const string ImagePendingEditHint = "This image has an unsaved edit — save the document before replacing it.";

    private ImageControls CurrentImageControls()
    {
        var active = _session is not null && ContentEditButton.IsEnabled && _contentEditMode;
        var image = active ? _selectedContentImage : null;
        return ImageCardRules.Resolve(
            selected: image is not null,
            pendingEdit: image is not null && _pendingImageEdits.Contains((image.PageIndex, image.Id)),
            canCheckpoint: _annotationState?.ContentEditsCanCheckpoint == true);
    }

    private void UpdateImageCard()
    {
        var controls = CurrentImageControls();
        var idle = !_isBusy && !_editingImage;
        // A pending move or resize folds a deletion into itself in the core,
        // so only Replace waits for a save — and only where the core may not
        // write the pending edit into a new base first.
        DeleteImageButton.IsEnabled = idle && controls is ImageControls.Ready or ImageControls.PendingEdit;
        ReplaceImageButton.IsEnabled = idle && controls == ImageControls.Ready;
        _editImageHint.Text = controls switch
        {
            ImageControls.Ready => ImageSelectedHint,
            ImageControls.PendingEdit => ImagePendingEditHint,
            _ => NoImageSelectedHint,
        };
        // The quick-action bar reads these buttons and the edit in flight.
        ScheduleQuickActions();
    }

    /// <summary>
    /// The selected image as the document stands now.
    /// </summary>
    /// <remarks>
    /// A <see cref="ContentImage"/> is bound to the revision that parsed it,
    /// and the page-content cache is dropped only for the page an edit
    /// touched — so an image selected on one page goes stale the moment
    /// anything is edited on another, and the facade refuses it. Re-reading the
    /// page's images and matching the same id at the same bounds gives a
    /// current handle; anything else means the page moved underneath the
    /// selection, and the honest answer is to ask for it again. A checkpoint
    /// renumbers the page it rewrites, so the one image at the same bounds is
    /// accepted too (<see cref="ImageCardRules.Current"/>).
    /// </remarks>
    private async Task<ContentImage?> CurrentContentImageAsync(string sessionId, ContentImage selected)
    {
        var images = await _facade.PageImagesAsync(sessionId, selected.PageIndex);
        if (_session?.SessionId != sessionId) return null;
        if (!images.IsSuccess)
        {
            AnnotationStatus.Text = images.Error!.Message;
            return null;
        }
        var current = ImageCardRules.Current(images.Value!, selected);
        if (current is null) AnnotationStatus.Text = "The page changed — select the image again.";
        return current;
    }

    private async void DeleteImageButton_Click(object sender, RoutedEventArgs e) => await DeleteSelectedImageAsync();

    private async Task DeleteSelectedImageAsync()
    {
        // A pending move or resize does not stop a deletion: the core folds it
        // into the queued edit, keeping the snapshot the file holds.
        if (_session is null || _organizing || _isBusy || _editingImage || _selectedContentImage is not { } selected) return;
        _editingImage = true;
        UpdateImageCard();
        var sessionId = _session.SessionId;
        var pageIndex = selected.PageIndex;
        try
        {
            await SettleContentEditorForHistoryAsync();
            if (_session?.SessionId != sessionId || !ReferenceEquals(_selectedContentImage, selected)) return;
            var current = await CurrentContentImageAsync(sessionId, selected);
            if (current is null || !ReferenceEquals(_selectedContentImage, selected)) return;
            var result = await _facade.RemoveImageAsync(sessionId, current);
            if (_session?.SessionId != sessionId) return;
            // An edit remains undoable even if rebuilding its preview failed.
            _contentEditedPages.Add(pageIndex);
            ClearContentImageSelection();
            AfterImageEdit(pageIndex);
            if (!result.IsSuccess)
            {
                await RefreshAnnotationStateAsync();
                AnnotationStatus.Text = result.Error!.Message;
                return;
            }
            _annotationState = result.Value;
            UpdateAnnotationControls(_annotationState);
            AnnotationStatus.Text = "Image deleted. Save to keep the change.";
        }
        finally
        {
            _editingImage = false;
            RedrawContentImageSelection();
        }
    }

    /// <summary>Drops what an image edit made stale on its page and re-renders it.</summary>
    private void AfterImageEdit(uint pageIndex)
    {
        CancelContentEditor();
        ForgetPageContent(pageIndex);
        foreach (var key in _pendingRunBounds.Keys.Where(key => key.Page == pageIndex).ToArray())
            _pendingRunBounds.Remove(key);
        InvalidatePageRender(pageIndex);
    }
}
