namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    /// <summary>
    /// The session serialized exactly as a save would write it, for the system
    /// share sheet to send elsewhere.
    /// </summary>
    /// <remarks>
    /// Unlike <see cref="SaveToDestinationAsync"/>, this does not touch
    /// <c>SavedRevision</c>: a copy sent to a phone is not this session saved,
    /// and closing must still ask about the pending edits. Signatures follow the
    /// save rule — <paramref name="signaturesAcknowledged"/> is <c>false</c> by
    /// default, so a caller that has not asked the reader gets a refusal.
    /// </remarks>
    public async Task<OperationResult<byte[]>> ShareBytesAsync(string sessionId, bool signaturesAcknowledged = false)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                {
                    return OperationResult<byte[]>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "share", sessionId, null));
                }
            }

            var bytes = await Task.Run(() => _core.SaveToBytes(session.Document, signaturesAcknowledged)).ConfigureAwait(false);
            return OperationResult<byte[]>.Success(bytes);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<byte[]>.Failure(MapError(error, "share", sessionId, null));
        }
        catch (Exception error)
        {
            return OperationResult<byte[]>.Failure(MapUnexpected(error, "share", sessionId, null));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }
}
