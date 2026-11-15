//! What the autosave timer decides on each tick, and what it refuses to write
//! at all — pure functions over the session, so the rules are tested without
//! a timer, a window or a disk.

use pdf_document::Document;

use crate::app::state::{DocumentSession, ImportedSource, SaveBacking};

/// Identifies the edit state a recovery snapshot was taken of.
///
/// All three numbers, because each moves on a different event: an open or a
/// rebuilding preview refresh installs a new session (`session_id`), every
/// reopen bumps `generation`, and every recorded command bumps
/// `edit_revision`. Writing again whenever any of them moved costs at most a
/// redundant snapshot; missing a move would leave an edit out of recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Revision {
    pub(crate) session_id: u64,
    pub(crate) generation: u64,
    pub(crate) edit_revision: u64,
}

/// What a tick sees of the open document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Observed {
    /// No document is open.
    Nothing,
    Document {
        revision: Revision,
        /// `DocumentSession::unsaved_to_disk`.
        unsaved: bool,
        /// See [`holds_encrypted_content`].
        encrypted: bool,
        /// Whether there is a model and a backing to serialize at all.
        saveable: bool,
        /// Whether the inline retype box is open.
        editing_text: bool,
    },
}

/// What a tick does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Delete any recovery entry this window holds: there is nothing unsaved
    /// left to protect, or nothing that may be written.
    Forget,
    /// Leave the entry as it is and look again next tick.
    Wait,
    /// Serialize the document and replace the entry.
    Write(Revision),
}

/// The tick's rule. `written` is the revision the entry on disk holds, if any.
pub(crate) fn decide(observed: Observed, written: Option<Revision>, in_flight: bool) -> Decision {
    // Checked first: a write that finishes after a newer decision would
    // overwrite it, so nothing is decided until it has landed.
    if in_flight {
        return Decision::Wait;
    }
    let Observed::Document {
        revision,
        unsaved,
        encrypted,
        saveable,
        editing_text,
    } = observed
    else {
        return Decision::Forget;
    };
    if !unsaved || encrypted || !saveable {
        return Decision::Forget;
    }
    if editing_text || written == Some(revision) {
        return Decision::Wait;
    }
    Decision::Write(revision)
}

/// Whether writing this session's bytes to the recovery directory could put
/// an encrypted document on disk in plaintext.
///
/// Recovery files are written without the user asking, to a directory they
/// never chose, and nothing the shell can do re-encrypts them: the save path
/// can only re-apply encryption when it holds every credential
/// (`pdf_save::full_rewrite_blocker`), and even then the bytes would need the
/// password to restore — a password this shell never stores. A document the
/// user protected with a password, or that came with restrictions, is
/// therefore never autosaved: losing unsaved work on a crash is the lesser
/// harm than leaving a decrypted copy of a protected file in
/// `~/.local/state`.
///
/// Encrypted means any of: the model carries a security context (the file had
/// an `/Encrypt` dictionary, which includes the "opens without a prompt but
/// restricts printing or copying" kind), a password was needed to open it,
/// the parsed base was decrypted on load, or a page was imported from a PDF
/// that was.
pub(crate) fn holds_encrypted_content(
    document: &Document,
    backing: &SaveBacking,
    imported: &[ImportedSource],
) -> bool {
    document.security.is_some()
        || backing.password.is_some()
        || backing.base.as_lopdf().encryption_state.is_some()
        || imported
            .iter()
            .any(|source| source.document.as_lopdf().encryption_state.is_some())
}

/// What a tick sees of `session`, read on the main thread.
pub(crate) fn observe(
    session: Option<&DocumentSession>,
    session_id: u64,
    generation: u64,
) -> Observed {
    let Some(session) = session else {
        return Observed::Nothing;
    };
    let model = session
        .document_model
        .as_ref()
        .zip(session.save_backing.as_ref());
    Observed::Document {
        revision: Revision {
            session_id,
            generation,
            edit_revision: session.edit_revision,
        },
        unsaved: session.unsaved_to_disk,
        // A session with nothing to serialize cannot leak anything either.
        encrypted: model.is_some_and(|(document, backing)| {
            holds_encrypted_content(document, backing, &session.imported_sources)
        }),
        saveable: model.is_some(),
        editing_text: session.content_editor.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_fixtures::{checkpointable_session, model_session, one_password_security};

    const REVISION: Revision = Revision {
        session_id: 1,
        generation: 2,
        edit_revision: 3,
    };

    fn dirty() -> Observed {
        Observed::Document {
            revision: REVISION,
            unsaved: true,
            encrypted: false,
            saveable: true,
            editing_text: false,
        }
    }

    fn with(change: impl FnOnce(&mut bool, &mut bool, &mut bool, &mut bool)) -> Observed {
        let Observed::Document {
            revision,
            mut unsaved,
            mut encrypted,
            mut saveable,
            mut editing_text,
        } = dirty()
        else {
            unreachable!()
        };
        change(
            &mut unsaved,
            &mut encrypted,
            &mut saveable,
            &mut editing_text,
        );
        Observed::Document {
            revision,
            unsaved,
            encrypted,
            saveable,
            editing_text,
        }
    }

    #[test]
    fn unsaved_work_not_yet_on_disk_is_written() {
        assert_eq!(decide(dirty(), None, false), Decision::Write(REVISION));
    }

    #[test]
    fn an_unchanged_document_is_not_written_again() {
        assert_eq!(decide(dirty(), Some(REVISION), false), Decision::Wait);
    }

    #[test]
    fn any_moved_counter_is_a_change() {
        for written in [
            Revision {
                session_id: 0,
                ..REVISION
            },
            Revision {
                generation: 0,
                ..REVISION
            },
            Revision {
                edit_revision: 0,
                ..REVISION
            },
        ] {
            assert_eq!(
                decide(dirty(), Some(written), false),
                Decision::Write(REVISION)
            );
        }
    }

    #[test]
    fn a_snapshot_still_being_written_is_never_raced() {
        assert_eq!(decide(dirty(), None, true), Decision::Wait);
        assert_eq!(decide(Observed::Nothing, None, true), Decision::Wait);
    }

    #[test]
    fn a_document_that_matches_disk_forgets_its_entry() {
        let clean = with(|unsaved, _, _, _| *unsaved = false);
        assert_eq!(decide(clean, Some(REVISION), false), Decision::Forget);
    }

    #[test]
    fn no_document_forgets_the_entry() {
        assert_eq!(
            decide(Observed::Nothing, Some(REVISION), false),
            Decision::Forget
        );
    }

    #[test]
    fn an_encrypted_document_is_never_written_and_forgets_any_entry() {
        let encrypted = with(|_, encrypted, _, _| *encrypted = true);
        assert_eq!(decide(encrypted, None, false), Decision::Forget);
    }

    #[test]
    fn a_document_with_nothing_to_serialize_forgets_its_entry() {
        let unsaveable = with(|_, _, saveable, _| *saveable = false);
        assert_eq!(decide(unsaveable, None, false), Decision::Forget);
    }

    /// The box's text is not in the model until it is committed, and
    /// committing it from a timer would close it under the user's fingers.
    /// The tick waits; the next one after the box closes sees the commit.
    #[test]
    fn an_open_retype_box_postpones_the_snapshot() {
        let typing = with(|_, _, _, editing| *editing = true);
        assert_eq!(decide(typing, None, false), Decision::Wait);
        assert_eq!(decide(typing, Some(REVISION), false), Decision::Wait);
    }

    #[test]
    fn a_plain_document_holds_no_encrypted_content() {
        let session = checkpointable_session(&["Hello"]);

        assert!(!holds_encrypted_content(
            session.document_model.as_ref().unwrap(),
            session.save_backing.as_ref().unwrap(),
            &[],
        ));
    }

    #[test]
    fn a_security_context_on_the_model_counts_as_encrypted() {
        let mut session = checkpointable_session(&["Hello"]);
        session.document_model.as_mut().unwrap().security = Some(one_password_security());

        assert!(holds_encrypted_content(
            session.document_model.as_ref().unwrap(),
            session.save_backing.as_ref().unwrap(),
            &[],
        ));
    }

    #[test]
    fn a_document_opened_with_a_password_counts_as_encrypted() {
        let mut session = checkpointable_session(&["Hello"]);
        session.save_backing.as_mut().unwrap().password = Some("fixture".to_owned());

        assert!(holds_encrypted_content(
            session.document_model.as_ref().unwrap(),
            session.save_backing.as_ref().unwrap(),
            &[],
        ));
    }

    #[test]
    fn observing_reads_the_session_flags() {
        let mut session = checkpointable_session(&["Hello"]);
        session.unsaved_to_disk = true;
        session.edit_revision = 3;

        assert_eq!(observe(Some(&session), 1, 2), dirty());
        assert_eq!(observe(None, 1, 2), Observed::Nothing);
    }

    #[test]
    fn a_session_without_a_backing_is_not_saveable() {
        let mut session = model_session(pdf_document::Document::blank());
        session.unsaved_to_disk = true;

        let Observed::Document { saveable, .. } = observe(Some(&session), 0, 0) else {
            panic!("a session is a document");
        };
        assert!(!saveable);
    }
}
