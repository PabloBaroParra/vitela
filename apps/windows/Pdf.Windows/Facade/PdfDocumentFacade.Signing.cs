namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    public async Task<OperationResult<SigningState>> SigningStateAsync(string sessionId)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<SigningState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "signing_state", sessionId, null));
            return OperationResult<SigningState>.Success(await Task.Run(() => new SigningState(sessionId,
                _core.SigningRefusal(session.Document), _core.ExtractSourceIsSigned(session.Document))).ConfigureAwait(false));
        }
        catch (PdfCoreException error) { return OperationResult<SigningState>.Failure(MapError(error, "signing_state", sessionId, null)); }
        catch (Exception error) { return OperationResult<SigningState>.Failure(MapUnexpected(error, "signing_state", sessionId, null)); }
        finally { _documentChangeGate.Release(); }
    }

    public async Task<OperationResult<ISigningCertificate>> OpenSigningCertificateAsync(byte[] bytes, string password)
    {
        try { return OperationResult<ISigningCertificate>.Success(await Task.Run(() => _core.OpenSigningCertificate(bytes, password)).ConfigureAwait(false)); }
        catch (PdfCoreException error) { return OperationResult<ISigningCertificate>.Failure(MapError(error, "certificate", null, null)); }
        catch (Exception error) { return OperationResult<ISigningCertificate>.Failure(MapUnexpected(error, "certificate", null, null)); }
    }

    public async Task<OperationResult<ISigningCertificate>> OpenTokenSigningSourceAsync(string modulePath, string? pin)
    {
        try { return OperationResult<ISigningCertificate>.Success(await Task.Run(() => _core.OpenTokenSigningSource(modulePath, pin)).ConfigureAwait(false)); }
        catch (PdfCoreException error) { return OperationResult<ISigningCertificate>.Failure(MapError(error, "token_certificate", null, null)); }
        catch (Exception error) { return OperationResult<ISigningCertificate>.Failure(MapUnexpected(error, "token_certificate", null, null)); }
    }

    public async Task<OperationResult<ISigningCertificate>> OpenSystemSigningSourceAsync()
    {
        try { return OperationResult<ISigningCertificate>.Success(await Task.Run(_core.OpenSystemSigningSource).ConfigureAwait(false)); }
        catch (PdfCoreException error) { return OperationResult<ISigningCertificate>.Failure(MapError(error, "system_certificate", null, null)); }
        catch (Exception error) { return OperationResult<ISigningCertificate>.Failure(MapUnexpected(error, "system_certificate", null, null)); }
    }

    /// <summary>Write and reopen under one document gate. A failed write never retires the live session.</summary>
    public async Task<OperationResult<DocumentSession>> SignToDestinationAsync(string sessionId, ISigningCertificate certificate,
        string identityId, string displayName, Func<byte[], Task> replaceDestination)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<DocumentSession>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "sign", sessionId, null));
            var refusal = _core.SigningRefusal(session.Document);
            if (refusal is not null)
                return OperationResult<DocumentSession>.Failure(CreateError(Sentence(refusal), PdfCoreError.UnsupportedOperation, "sign", sessionId, null));
            var bytes = await Task.Run(() => _core.SignToBytes(session.Document, certificate, identityId)).ConfigureAwait(false);
            await replaceDestination(bytes).ConfigureAwait(false);
            var document = await Task.Run(() => _core.ReopenSignedDocument(session.Document, bytes)).ConfigureAwait(false);
            var reopened = new SessionEntry(Guid.NewGuid().ToString("N"), displayName, document, _core.ContentEditingAllowed(document), (ulong)bytes.LongLength);
            lock (_gate)
            {
                RetireCurrentSessionLocked();
                _currentSession = reopened;
            }
            return OperationResult<DocumentSession>.Success(reopened.ToDto());
        }
        catch (PdfCoreException error) { return OperationResult<DocumentSession>.Failure(MapError(error, "sign", sessionId, null)); }
        catch (Exception error) { return OperationResult<DocumentSession>.Failure(MapUnexpected(error, "sign", sessionId, null)); }
        finally { _documentChangeGate.Release(); }
    }
}
