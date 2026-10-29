using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Facade;
using Windows.ApplicationModel.DataTransfer;

namespace Pdf.Windows;

/// <summary>
/// The content clipboard on the canvas (B25 T-209): in Edit content mode,
/// CTRL+C copies what is selected, CTRL+X cuts it, and CTRL+V pastes it with
/// its top-left corner where the pointer is — on any page, the same one or
/// another.
/// </summary>
/// <remarks>
/// The clip lives in this window, not on the system clipboard: it is a
/// one-page PDF only this app reads. A copy still puts the selected text on
/// the system clipboard, so it pastes as text anywhere else. The shell-wide
/// CTRL+C and CTRL+V accelerators ask <see cref="TryCopyContent"/> and
/// <see cref="TryPasteContent"/> first and fall back to copying page text
/// and pasting a bitmap when Edit content mode does not take the key.
/// </remarks>
public sealed partial class MainWindow
{
    /// <summary>What the last CTRL+C or CTRL+X copied, or <c>null</c>.</summary>
    private ContentClip? _contentClip;

    /// <summary>The page and point the pointer last crossed in Edit content mode, PDF space.</summary>
    private (uint Page, AnnotationPoint Point)? _contentPointer;

    private void TrackContentPointer(int pageIndex, AnnotationPoint point) => _contentPointer = ((uint)pageIndex, point);

    /// <summary>
    /// What a copy or cut acts on: the multi-selection, or the one block,
    /// image or shape selected on its own.
    /// </summary>
    private ContentSelection? ClipboardSelection()
    {
        if (_contentSelection is { IsEmpty: false } selection) return selection;
        if (_selectedTextBlock is { } block) return ContentSelection.Empty(block.PageIndex).Toggle(block);
        if (_selectedContentImage is { } image) return ContentSelection.Empty(image.PageIndex).Toggle(image);
        if (_selectedContentPath is { } path) return ContentSelection.Empty(path.PageIndex).Toggle(path);
        return null;
    }

    private bool ContentClipboardReady => _contentEditMode && _session is not null && !_isBusy && !_dialogOpen && _pump.Box is null;

    /// <summary>CTRL+C in Edit content mode with something selected; false leaves the key to text copy.</summary>
    private bool TryCopyContent()
    {
        if (!ContentClipboardReady || ClipboardSelection() is not { } selection) return false;
        _ = CopyContentAsync(selection, cut: false);
        return true;
    }

    /// <summary>CTRL+V in Edit content mode with a clip held; false leaves the key to the bitmap paste.</summary>
    private bool TryPasteContent()
    {
        if (!ContentClipboardReady || _contentClip is null) return false;
        _ = PasteContentAsync(_contentClip);
        return true;
    }

    private void CutContent_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        if (TextInputHasFocus() || !ContentClipboardReady || ClipboardSelection() is not { } selection)
        {
            args.Handled = false;
            return;
        }
        args.Handled = true;
        _ = CopyContentAsync(selection, cut: true);
    }

    private async Task CopyContentAsync(ContentSelection selection, bool cut)
    {
        if (_session is null) return;
        var sessionId = _session.SessionId;
        SetBusy(true);
        try
        {
            if (cut)
            {
                var result = await _facade.CutContentSelectionAsync(sessionId, selection);
                if (_session?.SessionId != sessionId) return;
                if (result.IsSuccess) Hold(result.Value!.Clip, selection);
                await AfterContentSelectionEditAsync(selection,
                    result.IsSuccess ? OperationResult<AnnotationState>.Success(result.Value!.State) : OperationResult<AnnotationState>.Failure(result.Error!),
                    "Cut. Point where it should go and press Ctrl+V.", keep: false);
                ClearSingleContentSelection();
            }
            else
            {
                var result = await _facade.CopyContentSelectionAsync(sessionId, selection);
                if (_session?.SessionId != sessionId) return;
                if (!result.IsSuccess)
                {
                    AnnotationStatus.Text = result.Error!.Message;
                    return;
                }
                Hold(result.Value!, selection);
                AnnotationStatus.Text = $"Copied {selection.Count} {(selection.Count == 1 ? "item" : "items")}. Point where it should go and press Ctrl+V.";
            }
        }
        finally
        {
            SetBusy(false);
        }
    }

    /// <summary>Keeps the clip for CTRL+V, and the selected text on the system clipboard for other apps.</summary>
    private void Hold(ContentClip clip, ContentSelection selection)
    {
        _contentClip = clip;
        var text = string.Join(Environment.NewLine, selection.Blocks.Select(block => block.Text));
        if (text.Length == 0) return;
        try
        {
            var package = new DataPackage();
            package.SetText(text);
            Clipboard.SetContent(package);
        }
        catch (Exception)
        {
            // The system clipboard is a convenience here; the clip is already held.
        }
    }

    /// <summary>
    /// Pastes where the pointer last was over a page; with no pointer seen,
    /// on the page in view at the place the clip was copied from.
    /// </summary>
    private async Task PasteContentAsync(ContentClip clip)
    {
        if (_session is not { } session || session.Pages.Count == 0) return;
        var (page, left, top) = _contentPointer is { } pointer && pointer.Page < session.Pages.Count
            ? (pointer.Page, pointer.Point.X, pointer.Point.Y)
            : ((uint)Math.Clamp(_firstVisiblePage, 0, session.Pages.Count - 1), clip.Bounds.X, clip.Bounds.Y + clip.Bounds.Height);
        var sessionId = session.SessionId;
        SetBusy(true);
        try
        {
            var result = await _facade.PasteContentAsync(sessionId, page, clip, left, top);
            if (_session?.SessionId != sessionId) return;
            var landed = new AnnotationRect(left, top - clip.Bounds.Height, clip.Bounds.Width, clip.Bounds.Height);
            await AfterContentPasteAsync(page, landed, result);
        }
        finally
        {
            SetBusy(false);
        }
    }

    /// <summary>
    /// Re-reads the page and selects what the paste added, so the next drag
    /// nudges the whole paste into place — still one undo step.
    /// </summary>
    private async Task AfterContentPasteAsync(uint page, AnnotationRect landed, OperationResult<AnnotationState> result)
    {
        if (!result.IsSuccess)
        {
            AnnotationStatus.Text = result.Error!.Message;
            return;
        }
        ClearSingleContentSelection();
        ClearContentSelection();
        await AfterContentSelectionEditAsync(ContentSelection.Empty(page), result,
            "Pasted. Drag it into place, or save to keep the change.", keep: false);
        if (_pageContent.TryGetValue(page, out var state) && state.Content is { } content
            && ContentSelection.Gather(content, landed, DrawnBounds).OnlyPending() is { IsEmpty: false } pasted)
        {
            SetContentSelection(pasted);
            AnnotationStatus.Text = $"Pasted {pasted.Count} {(pasted.Count == 1 ? "item" : "items")} — drag inside the dashed box to place them.";
        }
    }

    /// <summary>Lets go of a single-item selection a cut or paste made stale.</summary>
    private void ClearSingleContentSelection()
    {
        if (_selectedTextBlock is not null)
        {
            _selectedTextBlock = null;
            UpdateEditTextSelection();
        }
        if (_selectedContentImage is not null)
        {
            ClearContentImageSelection();
            RedrawContentImageSelection();
        }
        if (_selectedContentPath is not null) ClearContentPathSelection();
    }
}
