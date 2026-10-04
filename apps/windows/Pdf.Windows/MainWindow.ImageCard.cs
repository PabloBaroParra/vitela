using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>
/// The Images card's state: which of Replace and Delete the selected canvas
/// image allows, the sentence that says why, and the two commands themselves.
/// </summary>
/// <remarks>
/// Mirrors the GTK shell's <c>content_edit::ImageControls</c>. The state is
/// recomputed on every update rather than cached, because the one temporary
/// state (a pending edit) clears on save and reopen, and a remembered answer
/// would outlive it.
/// </remarks>
public sealed partial class MainWindow
{
    private enum ImageControls
    {
        /// <summary>No image selected, or nothing may be edited.</summary>
        Nothing,
        /// <summary>Selected, and both commands are available.</summary>
        Ready,
        /// <summary>Selected, but its bytes cannot be read back, so a swap could not be undone.</summary>
        NoReplace,
        /// <summary>Selected, and it already carries an unsaved edit. Outranks <see cref="NoReplace"/>.</summary>
        PendingEdit,
    }

    private const string NoImageSelectedHint = "Click an image on the page to select it.";
    private const string ImageSelectedHint = "Image selected — replace or delete it.";
    private const string ImageSelectedNoReplaceHint = "Image selected — move, resize or delete it. It cannot be replaced: "
        + "its current encoding cannot be read back, so the swap could not be undone.";
    private const string ImagePendingEditHint = "This image already has a pending edit — save and reopen before editing it again.";

    /// <summary>Images whose replacement the core refused before any picker opened.</summary>
    private readonly HashSet<(uint Page, ulong Image)> _replaceRefusedImages = [];

    private ImageControls CurrentImageControls()
    {
        if (_session is null || !ContentEditButton.IsEnabled || !_contentEditMode || _selectedContentImage is not { } image)
            return ImageControls.Nothing;
        var key = (image.PageIndex, image.Id);
        return _pendingImageEdits.Contains(key) ? ImageControls.PendingEdit
            : _replaceRefusedImages.Contains(key) ? ImageControls.NoReplace
            : ImageControls.Ready;
    }

    private void UpdateImageCard()
    {
        var controls = CurrentImageControls();
        var idle = !_isBusy && !_editingImage;
        DeleteImageButton.IsEnabled = idle && controls is ImageControls.Ready or ImageControls.NoReplace;
        ReplaceImageButton.IsEnabled = idle && controls == ImageControls.Ready;
        _editImageHint.Text = controls switch
        {
            ImageControls.Ready => ImageSelectedHint,
            ImageControls.NoReplace => ImageSelectedNoReplaceHint,
            ImageControls.PendingEdit => ImagePendingEditHint,
            _ => NoImageSelectedHint,
        };
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
    /// selection, and the honest answer is to ask for it again.
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
        var current = images.Value!.FirstOrDefault(image => image.Id == selected.Id && image.Bounds == selected.Bounds);
        if (current is null) AnnotationStatus.Text = "The page changed — select the image again.";
        return current;
    }

    /// <summary>
    /// Asks the core, as soon as an image is selected, whether it could be
    /// replaced — so an unrecoverable encoding greys Replace out and says why
    /// before anyone reaches for a file.
    /// </summary>
    private async Task ProbeImageReplacementAsync(ContentImage selected)
    {
        if (_session is null) return;
        var sessionId = _session.SessionId;
        var key = (selected.PageIndex, selected.Id);
        if (_replaceRefusedImages.Contains(key) || _pendingImageEdits.Contains(key)) return;
        var current = await CurrentContentImageAsync(sessionId, selected);
        if (current is null || !ReferenceEquals(_selectedContentImage, selected)) return;
        var probe = await _facade.PrepareImageReplacementAsync(sessionId, current);
        if (probe.IsSuccess || _session?.SessionId != sessionId || !ReferenceEquals(_selectedContentImage, selected)) return;
        _replaceRefusedImages.Add(key);
        UpdateImageCard();
    }

    private async void DeleteImageButton_Click(object sender, RoutedEventArgs e) => await DeleteSelectedImageAsync();

    private async Task DeleteSelectedImageAsync()
    {
        if (_session is null || _organizing || _isBusy || _editingImage || _selectedContentImage is not { } selected) return;
        if (CurrentImageControls() == ImageControls.PendingEdit)
        {
            AnnotationStatus.Text = ImagePendingEditHint;
            return;
        }
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
        _pageContent.Remove(pageIndex);
        foreach (var key in _pendingRunBounds.Keys.Where(key => key.Page == pageIndex).ToArray())
            _pendingRunBounds.Remove(key);
        InvalidatePageRender(pageIndex);
    }
}
