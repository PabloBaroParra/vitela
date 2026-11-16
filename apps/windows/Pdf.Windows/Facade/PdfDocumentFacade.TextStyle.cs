namespace Pdf.Windows.Facade;

/// <summary>
/// Bold, italic and underline toggles on existing text: what the buttons show
/// for a target — an open editor's runs, a selected block, a multi-selection —
/// and the restyle a press records, one undo step whatever the run count.
/// </summary>
/// <remarks>
/// What a restyle writes (a sibling face, or a stroked or sheared synthetic
/// one) and how it joins the queue are the core's; the facade only reads each
/// run's look, applies the toggle rule (<see cref="TextStylePolicy"/>) and
/// rebuilds the preview, as for a text block.
/// </remarks>
public sealed partial class PdfDocumentFacade
{
    /// <summary>
    /// What the toggles show for <paramref name="runs"/>: the first run's look,
    /// a restyle queued but not saved included. <c>null</c> when there are no
    /// runs or one's look cannot be read, which leaves the toggles off.
    /// </summary>
    public async Task<OperationResult<TextStyleButtons?>> TextStyleButtonsAsync(string sessionId, IReadOnlyList<ContentTextRun> runs)
    {
        var pageIndex = runs.Count > 0 ? runs[0].PageIndex : 0;
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<TextStyleButtons?>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "text_style_buttons", sessionId, pageIndex));
            }
            var states = await Task.Run(() => ReadLooks(session, runs)).ConfigureAwait(false);
            return OperationResult<TextStyleButtons?>.Success(states is null ? null : TextStylePolicy.Buttons(states));
        }
        catch (PdfCoreException error) { return OperationResult<TextStyleButtons?>.Failure(MapError(error, "text_style_buttons", sessionId, pageIndex)); }
        catch (Exception error) { return OperationResult<TextStyleButtons?>.Failure(MapUnexpected(error, "text_style_buttons", sessionId, pageIndex)); }
        finally { _documentChangeGate.Release(); }
    }

    /// <summary>
    /// Presses <paramref name="style"/> for <paramref name="runs"/>: on for all
    /// of them when the first run lacks it, off for all when it has it, runs
    /// already that way left out. Refused, before the core, when the style is
    /// the first run's own font with no plain sibling to go back to.
    /// </summary>
    public async Task<OperationResult<AnnotationState>> ToggleTextStyleAsync(string sessionId, IReadOnlyList<ContentTextRun> runs, TextStyleToggle style)
    {
        const string operation = "restyle_text";
        var pageIndex = runs.Count > 0 ? runs[0].PageIndex : 0;
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, operation, sessionId, pageIndex));
                if (!_core.ContentEditingAllowed(session.Document))
                    return OperationResult<AnnotationState>.Failure(CreateError("This document does not permit content changes.", PdfCoreError.UnsupportedOperation, operation, sessionId, pageIndex));
                if (runs.Count == 0)
                    return OperationResult<AnnotationState>.Success(session.AnnotationState(_core));

                if (ReadLooks(session, runs) is not { } states)
                    return OperationResult<AnnotationState>.Failure(CreateError("The style of this text cannot be read — save the document before restyling it.", PdfCoreError.UnsupportedOperation, operation, sessionId, pageIndex));
                var (restyles, refusal) = TextStylePolicy.Restyles([.. runs.Select((run, index) => (run.Source, states[index]))], style);
                if (refusal is not null)
                    return OperationResult<AnnotationState>.Failure(CreateError(refusal, PdfCoreError.UnsupportedOperation, operation, sessionId, pageIndex));
                if (restyles.Count == 0)
                    return OperationResult<AnnotationState>.Success(session.AnnotationState(_core));

                try
                {
                    _core.RestyleTextRuns(session.Document, restyles);
                    session.EditRevision++;
                    session.HasRecordedPreviewEdit = true;
                }
                catch (PdfCoreException error)
                {
                    return OperationResult<AnnotationState>.Failure(MapTextBlockError(error, operation, sessionId, pageIndex));
                }
            }
            return await RefreshPreviewAsync(session, operation, pageIndex).ConfigureAwait(false);
        }
        catch (PdfCoreException error) { return OperationResult<AnnotationState>.Failure(MapError(error, operation, sessionId, pageIndex)); }
        catch (Exception error) { return OperationResult<AnnotationState>.Failure(MapUnexpected(error, operation, sessionId, pageIndex)); }
        finally { _documentChangeGate.Release(); }
    }

    /// <summary>Each run's look, in order; <c>null</c> as soon as one cannot be read.</summary>
    private TextRunLookState[]? ReadLooks(SessionEntry session, IReadOnlyList<ContentTextRun> runs)
    {
        var states = new TextRunLookState[runs.Count];
        for (var index = 0; index < runs.Count; index++)
        {
            if (_core.ReadTextRunLook(session.Document, runs[index].Source) is not { } state) return null;
            states[index] = state;
        }
        return states;
    }
}
