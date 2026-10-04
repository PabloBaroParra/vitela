namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    // The shell errs toward prompting even after full Undo, like Linux. Other
    // facade clients retain the established clean-after-full-Undo contract.
    private bool HasLifecycleChanges(SessionEntry session) => _conservativeLifecycle
        ? session.EditRevision != session.SavedRevision
        : session.HasUnsavedEdits(_core);

    /// <summary>Uses the same authoritative guard as Open and CreateBlank.</summary>
    public async Task<OperationResult<bool>> HasUnsavedChangesAsync(string sessionId)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out var session))
                    return OperationResult<bool>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "unsaved_changes", sessionId, null));
                return OperationResult<bool>.Success(HasLifecycleChanges(session));
            }
        }
        catch (PdfCoreException error)
        {
            return OperationResult<bool>.Failure(MapError(error, "unsaved_changes", sessionId, null));
        }
        catch (Exception error)
        {
            return OperationResult<bool>.Failure(MapUnexpected(error, "unsaved_changes", sessionId, null));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }
}
