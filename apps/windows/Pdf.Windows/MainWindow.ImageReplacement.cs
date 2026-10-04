using Microsoft.UI.Xaml;

namespace Pdf.Windows;

/// <summary>
/// Replace image: swap the image selected on the canvas for a PNG or JPEG,
/// keeping its position and size.
/// </summary>
/// <remarks>
/// The core is asked before the picker opens (T-204): an image with a pending
/// edit, or one whose encoding cannot be read back for undo, is refused
/// without spending the reader's effort on choosing a file first.
/// </remarks>
public sealed partial class MainWindow
{
    private async void ReplaceImageButton_Click(object sender, RoutedEventArgs e)
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
            await SettleContentEditorForHistoryAsync();
            if (_session?.SessionId != sessionId || !ReferenceEquals(_selectedContentImage, selected)) return;
            var current = await CurrentContentImageAsync(sessionId, selected);
            if (current is null || !ReferenceEquals(_selectedContentImage, selected)) return;
            var prepared = await _facade.PrepareImageReplacementAsync(sessionId, current);
            if (_session?.SessionId != sessionId) return;
            if (!prepared.IsSuccess)
            {
                _replaceRefusedImages.Add(key);
                AnnotationStatus.Text = prepared.Error!.Message;
                return;
            }
            var bytes = await PickContentImageBytesAsync(sessionId);
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
            AnnotationStatus.Text = "Image replaced. Save to keep the change.";
        }
        finally
        {
            _editingImage = false;
            RedrawContentImageSelection();
        }
    }
}
