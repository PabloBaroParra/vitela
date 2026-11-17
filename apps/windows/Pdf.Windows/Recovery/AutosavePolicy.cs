using Pdf.Windows.Facade;

namespace Pdf.Windows.Recovery;

/// <summary>
/// Identifies the edit state a recovery snapshot was taken of. Every open
/// mints a new session id and every recorded command bumps the edit revision,
/// so either one moving means the entry on disk is out of date.
/// </summary>
public sealed record RecoveryRevision(string SessionId, ulong EditRevision);

public enum AutosaveDecision
{
    /// <summary>Delete any entry this window holds: nothing unsaved is left to protect, or nothing may be written.</summary>
    Forget,
    /// <summary>Leave the entry as it is and look again next tick.</summary>
    Wait,
    /// <summary>Serialize the document and replace the entry.</summary>
    Write,
}

/// <summary>What the autosave timer decides on each tick — pure, so the rule is tested without a timer, a window or a disk.</summary>
public static class AutosavePolicy
{
    /// <param name="observed">The open document, or <c>null</c> when none is.</param>
    /// <param name="editingText">
    /// Whether the inline retype box is open. Its text is not in the document
    /// until committed, and committing it from a timer would close it under the
    /// user's fingers, so the tick waits; the next one after it closes sees the
    /// commit.
    /// </param>
    /// <param name="written">The revision the entry on disk holds, if any.</param>
    public static AutosaveDecision Decide(RecoveryObservation? observed, bool editingText, RecoveryRevision? written)
    {
        // Encrypted documents are never written: the copy could not be
        // re-encrypted, and the shell never stores the password a protected
        // copy would need. Losing unsaved work on a crash is the lesser harm
        // than a decrypted copy of a protected file in the user's profile.
        if (observed is null || !observed.Unsaved || observed.Encrypted) return AutosaveDecision.Forget;
        if (editingText || written == new RecoveryRevision(observed.SessionId, observed.EditRevision)) return AutosaveDecision.Wait;
        return AutosaveDecision.Write;
    }
}
