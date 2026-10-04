namespace Pdf.Windows;

/// <summary>Deletion targets the open inline editor, never a different page's dialog choice.</summary>
public sealed partial class MainWindow
{
    private bool _deletingContentText;

    private async Task DeleteOpenContentTextAsync()
    {
        if (!DeleteTextButton.IsEnabled || _session is null || _pump.Box is not { } editor) return;
        var sessionId = _session.SessionId;
        _deletingContentText = true;
        editor.Box.IsReadOnly = true;
        _liveEdit.Stop();
        SetBusy(true);
        try
        {
            // A queued write must not arrive after the removal. Do not commit
            // unrecorded keystrokes: deletion supersedes them.
            await _pump.DrainAsync();
            if (_session?.SessionId != sessionId || _pump.Box != editor) return;
            if (_pump.WrittenFor(editor.PageIndex, editor.Run.Id) is not null)
            {
                AnnotationStatus.Text = "This text has a pending retype — save and reopen before deleting it.";
                return;
            }
            var targets = await _facade.PageTextEditTargetsAsync(sessionId, editor.PageIndex);
            if (_session?.SessionId != sessionId || _pump.Box != editor || !_contentEditMode || _organizing) return;
            if (!targets.IsSuccess)
            {
                AnnotationStatus.Text = targets.Error!.Message;
                return;
            }
            var target = targets.Value!.FirstOrDefault(run => run.Id == editor.Run.Id);
            if (target is null)
            {
                AnnotationStatus.Text = "The text changed. Click the run again before deleting it.";
                return;
            }
            var result = await _facade.RemoveTextRunAsync(sessionId, target);
            if (_session?.SessionId != sessionId) return;
            if (!result.IsSuccess)
            {
                var remaining = await _facade.PageTextEditTargetsAsync(sessionId, editor.PageIndex);
                if (_session?.SessionId != sessionId) return;
                // Keep the editable target on a refused command. A preview
                // failure after recording is different: the pending model no
                // longer contains the run, and history must remain reachable.
                if (!remaining.IsSuccess || remaining.Value!.Any(run => run.Id == target.Id))
                {
                    await RefreshAnnotationStateAsync();
                    AnnotationStatus.Text = result.Error!.Message;
                    return;
                }
            }
            // A failed preview can still leave a recorded, undoable deletion.
            CancelContentEditor();
            _contentEditedPages.Add(editor.PageIndex);
            _pageContent.Remove(editor.PageIndex);
            foreach (var key in _pendingRunBounds.Keys.Where(key => key.Page == editor.PageIndex).ToArray())
                _pendingRunBounds.Remove(key);
            if (editor.PageIndex < _slots.Count) ClearContentOutlines(_slots[(int)editor.PageIndex]);
            InvalidatePageCharacters(editor.PageIndex);
            InvalidatePageRender(editor.PageIndex);
            if (result.IsSuccess)
            {
                _annotationState = result.Value;
                UpdateAnnotationControls(_annotationState);
            }
            else await RefreshAnnotationStateAsync();
            RedrawAnnotations();
            AnnotationStatus.Text = result.IsSuccess ? "Text deleted. Save to keep the change." : result.Error!.Message;
        }
        finally
        {
            editor.Box.IsReadOnly = false;
            _deletingContentText = false;
            SetBusy(false);
            UpdateAnnotationControls(_annotationState);
        }
    }
}
