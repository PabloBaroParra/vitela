using Microsoft.UI.Xaml;
using Pdf.Windows.Viewer;
using Pdf.Windows.Localization;

namespace Pdf.Windows;

/// <summary>
/// Replace image: swap the image selected on the canvas for a PNG or JPEG,
/// keeping its position and size.
/// </summary>
/// <remarks>
/// The core is asked before the picker opens: an image with a pending edit in
/// a session that may not checkpoint is refused without spending the reader's
/// effort on choosing a file first. Anywhere else an image is replaced again
/// before a save: the core writes the queue into a new in-memory base when it
/// needs to, and the selection follows the image into it. Every encoding is
/// replaceable — undo restores the original exactly.
/// </remarks>
public sealed partial class MainWindow
{
    private async void ReplaceImageButton_Click(object sender, RoutedEventArgs e) =>
        await ReplaceSelectedImageAsync(PickContentImageBytesAsync);

    /// <summary>
    /// Replaces the selected image with what <paramref name="pickBytes"/>
    /// hands back — the file picker, or the smoke harness's picture.
    /// </summary>
    private async Task ReplaceSelectedImageAsync(Func<string, Task<byte[]?>> pickBytes)
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
        var key = (selected.PageIndex, selected.Id);
        try
        {
            if (!await SettleContentEditorForHistoryAsync()) return;
            if (_session?.SessionId != sessionId || !ReferenceEquals(_selectedContentImage, selected)) return;
            var current = await CurrentContentImageAsync(sessionId, selected);
            if (current is null || !ReferenceEquals(_selectedContentImage, selected)) return;
            var prepared = await _facade.PrepareImageReplacementAsync(sessionId, current);
            if (_session?.SessionId != sessionId) return;
            if (!prepared.IsSuccess)
            {
                AnnotationStatus.Text = prepared.Error!.Message;
                return;
            }
            var bytes = await pickBytes(sessionId);
            if (bytes is null || _session?.SessionId != sessionId) return;
            var result = await _facade.ReplaceContentImageAsync(sessionId, current, bytes);
            if (_session?.SessionId != sessionId) return;
            // A recorded edit remains undoable even when rebuilding its preview fails.
            _contentEditedPages.Add(pageIndex);
            AfterImageEdit(pageIndex);
            RedrawAnnotations();
            if (!result.IsSuccess)
            {
                await RefreshAnnotationStateAsync();
                AnnotationStatus.Text = result.Error!.Message;
                return;
            }
            _pendingImageEdits.Add(key);
            _annotationState = result.Value;
            UpdateAnnotationControls(_annotationState);
            AnnotationStatus.Text = I18n.T("edit-image-replaced-save");
            // A replacement that took a checkpoint renumbered the page: the
            // selection follows the image to where the new base holds it, so
            // the next Replace, drag or Delete names an image the core has.
            if (await EnsurePageContentAsync(pageIndex) is { } content
                && _session?.SessionId == sessionId && ReferenceEquals(_selectedContentImage, selected))
            {
                _selectedContentImage = ImageCardRules.Current(content.Images, selected);
                if (_selectedContentImage is { } followed) _pendingImageEdits.Add((followed.PageIndex, followed.Id));
            }
        }
        finally
        {
            _editingImage = false;
            RedrawContentImageSelection();
        }
    }
}
