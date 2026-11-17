namespace Pdf.Windows.Facade;

/// <summary>What the autosave timer sees of the open document on one tick.</summary>
/// <param name="EditRevision">Bumped by every recorded command, undo and redo included.</param>
/// <param name="Unsaved">The shell's unsaved-changes answer, the same one closing asks.</param>
/// <param name="Encrypted">See <see cref="IPdfCore.HoldsEncryptedContent"/>.</param>
public sealed record RecoveryObservation(string SessionId, ulong EditRevision, bool Unsaved, bool Encrypted);

/// <summary>A session serialized for its crash-recovery entry.</summary>
/// <param name="BreaksSignatures">
/// What Save would have asked before writing these bytes. Recorded because once
/// they are restored, the edit that broke the signature is in the base, where the
/// core can no longer see it.
/// </param>
public sealed record RecoverySnapshot(string SessionId, ulong EditRevision, byte[] Bytes, bool BreaksSignatures);

/// <summary>A recovered copy reopened, or why not.</summary>
/// <param name="Unrecoverable">
/// The failure was about the bytes themselves, so no later launch will open them
/// either and the entry may be discarded. Any other failure keeps it: deleting the
/// user's only copy over a passing fault is the failure to avoid.
/// </param>
public sealed record RecoveredOpen(OperationResult<DocumentSession> Result, bool Unrecoverable);

public sealed partial class PdfDocumentFacade
{
    /// <summary>The open document as the autosave timer sees it, or <c>null</c> when none is open.</summary>
    public async Task<RecoveryObservation?> ObserveForRecoveryAsync()
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry? session;
            ulong revision;
            bool unsaved;
            lock (_gate)
            {
                session = _currentSession;
                if (session is null) return null;
                revision = session.EditRevision;
                unsaved = HasLifecycleChanges(session);
            }

            // Off the calling thread: the handle's lock is shared with renders.
            // An unanswered question is treated as protected: nothing is written for it.
            var encrypted = await Task.Run(() =>
            {
                try { return _core.HoldsEncryptedContent(session.Document); }
                catch (PdfCoreException) { return true; }
            }).ConfigureAwait(false);
            return new RecoveryObservation(session.Id, revision, unsaved, encrypted);
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    /// <summary>
    /// The bytes a Save of <paramref name="sessionId"/> would write, for its
    /// crash-recovery entry.
    /// </summary>
    /// <remarks>
    /// Like <see cref="ShareBytesAsync"/>, this does not touch <c>SavedRevision</c>:
    /// a recovery copy is not the session saved. Signatures are acknowledged
    /// silently — the copy is not the user's document — and what the user would
    /// have been asked is returned instead. An unanswered question counts as a
    /// broken signature.
    /// </remarks>
    public async Task<OperationResult<RecoverySnapshot>> RecoverySnapshotAsync(string sessionId)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            ulong revision;
            bool baseBreaksSignatures;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                {
                    return OperationResult<RecoverySnapshot>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "recovery_snapshot", sessionId, null));
                }
                revision = session.EditRevision;
                baseBreaksSignatures = session.BaseBreaksSignatures;
            }

            var (bytes, breaks) = await Task.Run(() =>
            {
                bool breaksSignatures;
                try
                {
                    breaksSignatures = baseBreaksSignatures || _core.WillInvalidateSignatures(session.Document);
                }
                catch (PdfCoreException)
                {
                    breaksSignatures = true;
                }
                return (_core.SaveToBytes(session.Document, signaturesAcknowledged: true), breaksSignatures);
            }).ConfigureAwait(false);
            return OperationResult<RecoverySnapshot>.Success(new RecoverySnapshot(session.Id, revision, bytes, breaks));
        }
        catch (PdfCoreException error)
        {
            return OperationResult<RecoverySnapshot>.Failure(MapError(error, "recovery_snapshot", sessionId, null));
        }
        catch (Exception error)
        {
            return OperationResult<RecoverySnapshot>.Failure(MapUnexpected(error, "recovery_snapshot", sessionId, null));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    /// <summary>
    /// Opens a recovered copy as the unsaved document it was: under its original
    /// name, reporting unsaved changes until it is saved, and — when
    /// <paramref name="breaksSignatures"/> — still warning that a save breaks the
    /// signature the original carries.
    /// </summary>
    /// <remarks>
    /// The undo history is not recovered: the core has no serialization of the
    /// edit log, so the recovered session starts a fresh one. The ordinary
    /// unsaved-changes guard applies; a restore never replaces unsaved work.
    /// </remarks>
    public async Task<RecoveredOpen> OpenRecoveredAsync(DocumentSource source, bool breaksSignatures)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            lock (_gate)
            {
                if (_currentSession is { } current && HasLifecycleChanges(current))
                {
                    return new RecoveredOpen(OperationResult<DocumentSession>.Failure(CreateError(
                        "Save or discard the unsaved changes before restoring another document.", PdfCoreError.UnsavedChanges, "open_recovered", current.Id, null)), Unrecoverable: false);
                }
            }

            var document = await Task.Run(() => _core.OpenFromBytes(source.Bytes, null)).ConfigureAwait(false);
            var session = new SessionEntry(Guid.NewGuid().ToString("N"), source.DisplayName, document, _core.ContentEditingAllowed(document), (ulong)source.Bytes.LongLength)
            {
                // Every edit the bytes hold is unsaved; none is in the new history.
                EditRevision = 1,
                BaseBreaksSignatures = breaksSignatures,
            };
            lock (_gate)
            {
                RetireCurrentSessionLocked();
                _currentSession = session;
            }

            return new RecoveredOpen(OperationResult<DocumentSession>.Success(session.ToDto()), Unrecoverable: false);
        }
        catch (PdfCoreException error)
        {
            return new RecoveredOpen(OperationResult<DocumentSession>.Failure(MapError(error, "open_recovered", null, null)), IsAboutTheBytes(error.Category));
        }
        catch (Exception error)
        {
            return new RecoveredOpen(OperationResult<DocumentSession>.Failure(MapUnexpected(error, "open_recovered", null, null)), Unrecoverable: false);
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    /// <summary>
    /// Whether an open failure says the bytes can never be opened. A password
    /// failure qualifies because autosave never writes an encrypted copy, and
    /// <see cref="PdfCoreError.Internal"/> because on the open path it is the
    /// parser refusing the bytes. A render failure does not: the core reports a
    /// renderer that cannot load and pdfium refusing a format the same way, and
    /// only the second is about the bytes.
    /// </summary>
    private static bool IsAboutTheBytes(PdfCoreError category) => category is
        PdfCoreError.PasswordRequired or PdfCoreError.WrongPassword or PdfCoreError.UnsupportedSecurityHandler or PdfCoreError.Internal;
}
