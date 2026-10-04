using Pdf.Windows.Facade;
using Windows.Security.Cryptography;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>
/// "Insert image" armed and a page clicked: pick a PNG or JPEG and add it as
/// page content with its top-left corner at the click.
/// </summary>
/// <remarks>
/// The default box is the annotation Stamp tool's: natural proportions, the
/// longest side capped at 144 pt (<c>stamp_placement</c>, applied inside the
/// facade). The GTK shell's <c>image::insert_at</c> reaches the same core
/// function, so the two shells agree about where a placed image lands.
/// </remarks>
public sealed partial class MainWindow
{
    private async Task InsertContentImageAtAsync(uint pageIndex, AnnotationPoint point)
    {
        if (_session is null || _organizing || _isBusy || _editingImage) return;
        _editingImage = true;
        var sessionId = _session.SessionId;
        try
        {
            var prepared = await _facade.PrepareImageInsertionAsync(sessionId, pageIndex);
            if (_session?.SessionId != sessionId) return;
            if (!prepared.IsSuccess) { AnnotationStatus.Text = prepared.Error!.Message; return; }
            var bytes = await PickContentImageBytesAsync(sessionId);
            if (bytes is null || _session?.SessionId != sessionId) return;
            var result = await _facade.InsertContentImageAsync(sessionId, prepared.Value!, bytes, point.X, point.Y);
            if (_session?.SessionId != sessionId) return;
            // A recorded edit remains undoable even when refreshing its preview fails.
            _contentEditedPages.Add(pageIndex);
            CancelContentEditor();
            _pageContent.Remove(pageIndex);
            if (pageIndex < _slots.Count) ClearContentOutlines(_slots[(int)pageIndex]);
            InvalidatePageRender(pageIndex);
            RedrawAnnotations();
            if (!result.IsSuccess)
            {
                await RefreshAnnotationStateAsync();
                AnnotationStatus.Text = result.Error!.Message;
                return;
            }
            _annotationState = result.Value;
            UpdateAnnotationControls(_annotationState);
            AnnotationStatus.Text = "Image inserted. Save to keep the change.";
        }
        finally { _editingImage = false; }
    }

    /// <summary>
    /// Asks for a PNG or JPEG and reads it, or <c>null</c> when the picker was
    /// cancelled or the file could not be read (the reason goes to the status
    /// line). Shared with Replace image.
    /// </summary>
    private async Task<byte[]?> PickContentImageBytesAsync(string sessionId)
    {
        var picker = new FileOpenPicker();
        foreach (var extension in new[] { ".png", ".jpg", ".jpeg" }) picker.FileTypeFilter.Add(extension);
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        var file = await picker.PickSingleFileAsync();
        if (file is null || _session?.SessionId != sessionId) return null;
        try
        {
            var buffer = await FileIO.ReadBufferAsync(file);
            CryptographicBuffer.CopyToByteArray(buffer, out var bytes);
            return bytes;
        }
        catch (Exception)
        {
            if (_session?.SessionId == sessionId) AnnotationStatus.Text = "The selected image could not be read.";
            return null;
        }
    }
}
