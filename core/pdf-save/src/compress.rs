//! When a save compresses what it wrote, and what it is allowed to compress
//! (Batch 24, T-195).
//!
//! ## The rule this module exists to keep
//!
//! *`pdf-compress` decides how hard to squeeze; this module decides whether
//! it gets to run at all.*
//!
//! That split is `docs/batch-compress.md`'s responsibility boundary, and it
//! is why compression arrives as a second entry point next to
//! [`crate::save_document`] rather than as a stage inside the writer: the
//! quality tables, the resampler, the prune and the never-grow guarantee are
//! all `pdf-compress`'s, and none of them are things a save pipeline should
//! have opinions about.
//!
//! ## Why compression runs *after* the writer, on bytes
//!
//! A compressed document is one written with object streams and a
//! cross-reference stream, and `lopdf` decides that at serialisation time.
//! Handing the working graph to `pdf-compress` mid-save and then serialising
//! it here would throw the repack away: [`crate::strategy`]'s final
//! `save_to` is the classic writer, so the file would come back out with
//! every object loose again. Compression has to be the last thing that
//! produces bytes, which makes it a byte-to-byte pass over a finished save.
//!
//! The cost is one extra parse and one extra serialise per compressed save.
//! It buys the property that matters more: a compression that declines
//! returns the save's own bytes, so the failure mode of this whole feature is
//! "the ordinary save you would have got anyway".
//!
//! ## No `Command`, no `EditLog`
//!
//! Decision 2, and it is why this is a save-time entry point rather than an
//! edit: the inverse of "re-encoded four hundred images" is a second copy of
//! the file. Nothing here touches `Document::pending_edits`, so compressing
//! creates no undo step — undoing a compression is declining to save it.
//!
//! ## The two gates, and why only one of them has a yes
//!
//! **Signatures have a yes.** A rewrite invalidates them, the user is the one
//! who owns that decision, and this crate already has the channel for it:
//! [`SignatureAcknowledgement`] on the save input, warned about ahead of time
//! by [`compressed_save_will_invalidate_signatures`]. A compressed save is
//! always a full rewrite (see `crate::strategy`'s `Compression`), so the
//! existing refusal fires even for a save that edits nothing — which is
//! exactly the case a "just compress this file" button produces. Acknowledged,
//! the same value travels on as [`SignedDocuments::CompressAnyway`] so that
//! `pdf-compress` does not refuse a second time on the user's behalf.
//!
//! **Encryption has no yes, and this is where `docs/batch-compress.md`
//! decision 7 does not survive contact with the writer.** The decision says
//! to ask [`crate::full_rewrite_blocker`], the gate that answers whether a
//! protected document can be rewritten with its protection intact — the
//! implication being that a document whose passwords we hold could be
//! compressed. It cannot, and the reason is upstream of any policy: `lopdf`'s
//! `save_with_object_streams` returns early for an encrypted document and
//! writes every object loose, because the objects are already encrypted by
//! the time the writer sees them and the file encryption key is gone. There
//! is no way to ask for the repack and get it. Approaching from the other
//! side gives the same answer — `pdf-compress` cannot *read* an encrypted
//! document either (`Document::load_mem` returns a handle holding nothing but
//! `/Encrypt`), which is why it carries the same refusal itself.
//!
//! So [`compression_blocker`] answers the narrower question the writer
//! actually leaves room for: *will these bytes come out encrypted?* It reads
//! the same two fields `crate::security::apply_encryption_for_full_rewrite`
//! reads, so the two cannot disagree about what is about to be written.
//! `full_rewrite_blocker` is still consulted on this save — by
//! `crate::security::build_encryption_state`, on the writer's own path — and
//! calling it a second time here would add a reason without adding an answer.
//!
//! The yes-path for a protected document is therefore
//! [`SaveIntent::StripProtection`]: a separately consented operation that
//! produces plaintext, which compresses like anything else. That is a
//! different thing to ask a user than "compress anyway", and it should be.

use pdf_compress::{CompressPreset, CompressReport, Refusal, SignedDocuments};

use crate::error::SaveError;
use crate::security::SaveIntent;
use crate::strategy::{
    self, AnnotationLayer, Compression, SaveInput, SaveOptions, SaveOutcome,
    SignatureAcknowledgement,
};

/// A save that went through [`pdf_compress`], and what that did to it.
///
/// [`bytes`](Self::bytes) are always the ones to write: a compression that
/// found nothing to gain returns the save's own output rather than an
/// equivalent-but-different re-serialisation of it, and the report says
/// [`Outcome::NoGain`](pdf_compress::Outcome::NoGain). The report travels
/// with the bytes for the same reason `pdf_compress::Compressed` is shaped
/// that way — a caller cannot take one without seeing the other.
#[derive(Debug, Clone)]
pub struct CompressedSave {
    /// The document to write out.
    pub bytes: Vec<u8>,
    /// What compression did to it, measured against the bytes the save
    /// produced — not against the file the user originally opened.
    pub report: CompressReport,
    /// Whatever the graft had to report, exactly as [`crate::SaveOutcome`]
    /// carries it. Compression does not consume or change these.
    pub graft_warnings: Vec<pdf_manip::GraftWarning>,
}

/// Why this save's bytes cannot be compressed, or `None` when they can.
///
/// Answers "will these bytes come out encrypted?" — see this module's header
/// for why that, and not `full_rewrite_blocker`, is the question compression
/// turns on. Cheap on purpose: it decides the writer, so it has to be
/// answerable before the save runs rather than after it.
///
/// `pdf-compress` refuses an encrypted document independently, so this is an
/// optimisation of the refusal rather than the only thing standing between a
/// protected file and a repack. What it saves is real, though: a blocked save
/// stays on whichever writer it would have used, instead of being forced into
/// a full rewrite and a parse of the result for a compression that was never
/// going to happen.
pub fn compression_blocker(input: SaveInput<'_>) -> Option<Refusal> {
    // Stripping produces plaintext, whatever the document arrived as — and
    // `full_rewrite_blocker` deliberately says nothing about this intent.
    if input.intent == SaveIntent::StripProtection {
        return None;
    }

    input
        .document
        .security
        .as_ref()
        .map(|_| Refusal::EncryptedDocumentNotRewritable)
}

/// Whether compressing this save breaks a signature the file carries.
///
/// The counterpart to [`crate::will_invalidate_signatures`], and a shell
/// should ask this one before offering a compression: a compressed save is a
/// full rewrite even when nothing was edited, so a signed file answers `true`
/// here where the ordinary query answers `false`. Answering `true` is exactly
/// the condition under which [`save_document_compressed`] returns
/// [`SaveError::SignaturesWouldBeInvalidated`] for an
/// [`SignatureAcknowledgement::Unacknowledged`] input.
///
/// A save whose compression is blocked falls back to the ordinary question,
/// because that is the save that will actually run.
///
/// # Errors
///
/// Whatever [`crate::will_invalidate_signatures`] returns when it has to walk
/// the base document to answer.
pub fn compressed_save_will_invalidate_signatures(input: SaveInput<'_>) -> Result<bool, SaveError> {
    if compression_blocker(input).is_some() {
        return strategy::will_invalidate_signatures(input);
    }

    Ok(pdf_manip::document_has_signatures(input.base))
}

/// Saves `input` and compresses the result as far as `preset` allows.
///
/// [`crate::save_document`] stays the entry point for an ordinary save; this
/// one is for a caller that asked for a smaller file. The bytes it returns
/// are never larger than the ones the save produced — that guarantee is
/// `pdf-compress`'s and is structural there, not a check made here.
///
/// # Errors
///
/// Everything [`crate::save_document`] can return, plus
/// [`SaveError::Compress`] if the bytes this crate just wrote cannot be read
/// back. In particular [`SaveError::SignaturesWouldBeInvalidated`] when the
/// file is signed and `input.signatures` has not settled it — see
/// [`compressed_save_will_invalidate_signatures`].
pub fn save_document_compressed(
    input: SaveInput<'_>,
    preset: CompressPreset,
) -> Result<CompressedSave, SaveError> {
    let blocker = compression_blocker(input);
    let compression = match blocker {
        // Nothing is going to be compressed, so nothing should be paid for
        // it: the save takes the writer it would have taken anyway.
        Some(_) => Compression::None,
        None => Compression::Requested,
    };

    let SaveOutcome {
        bytes,
        graft_warnings,
    } = strategy::save_with_layer(
        input,
        SaveOptions::default(),
        AnnotationLayer::Materialize,
        compression,
    )?;

    if let Some(refusal) = blocker {
        let size = bytes.len() as u64;
        return Ok(CompressedSave {
            bytes,
            report: CompressReport::refused(size, refusal),
            graft_warnings,
        });
    }

    let compressed = pdf_compress::compress(&bytes, preset, signed_documents(input.signatures))?;

    Ok(CompressedSave {
        report: compressed.report().clone(),
        bytes: compressed.into_bytes(),
        graft_warnings,
    })
}

/// The acknowledgement, in the vocabulary of the crate that acts on it.
///
/// Two names for one decision, because neither crate may depend on the
/// other's: `pdf-compress` knows nothing about saving, and this is the one
/// place the two words are put side by side. Written as a total `match` so a
/// variant added to either enum has to come back here.
fn signed_documents(acknowledgement: SignatureAcknowledgement) -> SignedDocuments {
    match acknowledgement {
        SignatureAcknowledgement::Unacknowledged => SignedDocuments::LeaveAlone,
        SignatureAcknowledgement::ProceedAndInvalidate => SignedDocuments::CompressAnyway,
    }
}

#[cfg(test)]
mod tests {
    use pdf_document::{
        Credential, EncryptionCredentials, Permissions, SecurityContext, SecurityHandler,
    };

    use super::*;
    use crate::bridge::{self, ImportedSources};

    fn protected() -> SecurityContext {
        SecurityContext {
            handler: SecurityHandler::Rc4_128,
            credential: Credential::User,
            credentials: EncryptionCredentials::both("user-pw", "owner-pw"),
            permissions: Permissions(0xFFFF_FFFC_u32),
        }
    }

    struct Fixture {
        document: pdf_document::Document,
        base: pdf_manip::LopdfDocument,
        intent: SaveIntent,
    }

    impl Fixture {
        fn plain() -> Self {
            let base = pdf_manip::create_blank_document(
                pdf_document::PageSize::A4,
                pdf_document::Orientation::Portrait,
            );
            let document = bridge::document_from_lopdf(&base, None).unwrap();
            Fixture {
                document,
                base,
                intent: SaveIntent::Default,
            }
        }

        fn input(&self) -> SaveInput<'_> {
            SaveInput {
                document: &self.document,
                base: &self.base,
                original_bytes: None,
                intent: self.intent,
                signatures: SignatureAcknowledgement::Unacknowledged,
                imported_sources: ImportedSources::none(),
            }
        }
    }

    #[test]
    fn an_unprotected_save_has_nothing_blocking_its_compression() {
        assert_eq!(compression_blocker(Fixture::plain().input()), None);
    }

    /// The gate. Protection that this save will re-apply means bytes
    /// `pdf-compress` can neither read nor repack — see this module's header.
    #[test]
    fn a_save_that_re_applies_protection_cannot_be_compressed() {
        let mut fixture = Fixture::plain();
        fixture.document.security = Some(protected());

        assert_eq!(
            compression_blocker(fixture.input()),
            Some(Refusal::EncryptedDocumentNotRewritable)
        );
    }

    /// The yes-path for a protected document, and the reason the gate is
    /// about the *output* rather than about the document: a strip writes
    /// plaintext, and plaintext compresses.
    #[test]
    fn stripping_protection_leaves_the_compression_free_to_run() {
        let mut fixture = Fixture::plain();
        fixture.document.security = Some(protected());
        fixture.intent = SaveIntent::StripProtection;

        assert_eq!(compression_blocker(fixture.input()), None);
    }

    /// The translation, both ways round, so that neither default drifts into
    /// meaning the other crate's opposite.
    #[test]
    fn the_acknowledgement_crosses_the_boundary_unchanged() {
        assert_eq!(
            signed_documents(SignatureAcknowledgement::Unacknowledged),
            SignedDocuments::LeaveAlone
        );
        assert_eq!(
            signed_documents(SignatureAcknowledgement::ProceedAndInvalidate),
            SignedDocuments::CompressAnyway
        );
        assert_eq!(
            signed_documents(SignatureAcknowledgement::default()),
            SignedDocuments::default(),
            "a caller who said nothing must land on the safe value on both sides"
        );
    }
}
