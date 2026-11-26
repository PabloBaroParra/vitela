//! Compressing an open document (B26 T-250): the presets a dialog offers,
//! the two questions it asks before it runs, and the compressed save.
//!
//! Every rule is the core's, asked the way `pdf-ffi`'s `compress` module
//! asks it: `pdf_save::compression_blocker` (an encrypted document cannot be
//! repacked), `pdf_save::compressed_save_will_invalidate_signatures` (not
//! the ordinary signature question: a compressed save is always a full
//! rewrite, so a signed file answers `true` even with nothing edited) and
//! `pdf_save::save_document_compressed`, over the same `SaveInput` an
//! ordinary save builds — pending edits included. The web keeps the
//! document's encryption on every save (`SaveIntent::Default`), so there is
//! no "strip protection" way past the encryption refusal here, exactly as on
//! Linux.
//!
//! Compressing is not an edit: it records no undo step and leaves the
//! session as it found it (`docs/batch-compress.md` decision 2).

use pdf_compress::{CompressPreset, Outcome, Refusal};

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// Every preset a compress dialog may offer, by the name it crosses the
/// boundary under, strongest-preserving first — the core's own list
/// (`CompressPreset::all`), so the web cannot drift into a fourth.
pub fn compress_presets() -> Vec<&'static str> {
    CompressPreset::all().into_iter().map(preset_name).collect()
}

/// The name a preset crosses the boundary under.
pub fn preset_name(preset: CompressPreset) -> &'static str {
    // Total, no wildcard: a fourth preset has to be named here.
    match preset {
        CompressPreset::Lossless => "lossless",
        CompressPreset::Balanced => "balanced",
        CompressPreset::Small => "small",
    }
}

/// The preset `name` names, or `UnsupportedOperation` for a name no preset
/// has.
pub fn preset_from_name(name: &str) -> Result<CompressPreset, EngineError> {
    CompressPreset::all()
        .into_iter()
        .find(|preset| preset_name(*preset) == name)
        .ok_or_else(|| {
            EngineError::new(
                ErrorKind::UnsupportedOperation,
                format!("'{name}' is not a compression preset"),
            )
        })
}

/// Something a compression could not do, and why: `kind` for the UI to
/// branch on, `message` in the core's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressRefusal {
    /// `encryptedDocumentNotRewritable`, `signaturesWouldBeInvalidated`, or
    /// `other` for a refusal this boundary does not model yet (the core's
    /// list is open-ended; its message still reaches the reader).
    pub kind: &'static str,
    pub message: String,
}

impl From<&Refusal> for CompressRefusal {
    fn from(refusal: &Refusal) -> Self {
        let kind = match refusal {
            Refusal::EncryptedDocumentNotRewritable => "encryptedDocumentNotRewritable",
            Refusal::SignaturesWouldBeInvalidated => "signaturesWouldBeInvalidated",
            _ => "other",
        };
        CompressRefusal {
            kind,
            message: refusal.to_string(),
        }
    }
}

/// A compressed save: the bytes, and what compression did to them —
/// `pdf-ffi`'s `FfiCompressedSave`, flattened. The sizes are the core's
/// (`before_bytes` is this save's own output, not the opened file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressedCopy {
    /// Never larger than the ordinary save; exactly it when nothing smaller
    /// came out or the compression was refused.
    pub bytes: Vec<u8>,
    pub before_bytes: u64,
    pub after_bytes: u64,
    pub saved_bytes: u64,
    /// Whether `after_bytes < before_bytes` (the core's `Outcome`).
    pub reduced: bool,
    /// What could not be done, in the order the core found it.
    pub refusals: Vec<CompressRefusal>,
}

impl From<pdf_save::CompressedSave> for CompressedCopy {
    // The graft warnings are dropped, as `pdf-ffi` drops them (and as
    // `save_to_bytes` here does).
    fn from(saved: pdf_save::CompressedSave) -> Self {
        let report = saved.report;
        CompressedCopy {
            before_bytes: report.before(),
            after_bytes: report.after(),
            saved_bytes: report.saved_bytes(),
            reduced: report.outcome() == Outcome::Reduced,
            refusals: report.refusals().iter().map(Into::into).collect(),
            bytes: saved.bytes,
        }
    }
}

impl DocumentModel {
    /// Why this document cannot be compressed, or `None` when it can — asked
    /// before the dialog offers to run. Cheap: it reads the protection, it
    /// saves nothing.
    pub fn compression_refusal(&self) -> Option<CompressRefusal> {
        self.with_save_input(false, pdf_save::compression_blocker)
            .as_ref()
            .map(Into::into)
    }

    /// Whether compressing breaks a signature the file carries: when `true`,
    /// `save_compressed` refuses with `SignaturesWouldBeInvalidated` unless
    /// told the user agreed.
    pub fn compressed_save_will_invalidate_signatures(&self) -> Result<bool, EngineError> {
        Ok(self.with_save_input(false, pdf_save::compressed_save_will_invalidate_signatures)?)
    }

    /// Saves the document with every edit and compresses the result as far
    /// as `preset` allows. A refused compression is not an error: it comes
    /// back as the ordinary save's bytes with the reason in `refusals`.
    pub fn save_compressed(
        &self,
        preset: CompressPreset,
        invalidate_signatures: bool,
    ) -> Result<CompressedCopy, EngineError> {
        let saved = self.with_save_input(invalidate_signatures, |input| {
            pdf_save::save_document_compressed(input, preset)
        })?;
        Ok(saved.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{pdfs, PageEdit};
    use pdf_manip::OpenCredentials;

    fn open(bytes: &[u8]) -> DocumentModel {
        DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
    }

    fn page_count(bytes: &[u8]) -> usize {
        lopdf::Document::load_mem(bytes)
            .expect("the compressed copy parses")
            .get_pages()
            .len()
    }

    #[test]
    fn the_presets_are_the_cores_in_its_order_and_round_trip_by_name() {
        assert_eq!(compress_presets(), ["lossless", "balanced", "small"]);
        for preset in CompressPreset::all() {
            assert_eq!(preset_from_name(preset_name(preset)).unwrap(), preset);
        }
        let err = preset_from_name("tiny").unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsupportedOperation);
        assert!(err.message.contains("tiny"), "{}", err.message);
    }

    #[test]
    fn a_scan_comes_out_smaller_with_a_report_describing_the_bytes() {
        let model = open(pdfs::SCAN_200DPI);
        assert_eq!(model.compression_refusal(), None);
        assert!(!model.compressed_save_will_invalidate_signatures().unwrap());

        let copy = model
            .save_compressed(CompressPreset::Balanced, false)
            .expect("compresses");
        assert!(copy.reduced);
        assert!(copy.refusals.is_empty());
        assert_eq!(copy.after_bytes, copy.bytes.len() as u64);
        assert_eq!(copy.saved_bytes, copy.before_bytes - copy.after_bytes);
        assert!(
            copy.bytes.len() < pdfs::SCAN_200DPI.len(),
            "smaller than the opened file too: {} of {} bytes",
            copy.bytes.len(),
            pdfs::SCAN_200DPI.len()
        );
        assert_eq!(page_count(&copy.bytes), 1);
    }

    #[test]
    fn a_stronger_preset_never_comes_out_larger() {
        let model = open(pdfs::SCAN_200DPI);
        let sizes: Vec<u64> = CompressPreset::all()
            .into_iter()
            .map(|preset| model.save_compressed(preset, false).unwrap().after_bytes)
            .collect();
        assert!(
            sizes[0] >= sizes[1] && sizes[1] >= sizes[2],
            "lossless, balanced, small: {sizes:?}"
        );
    }

    #[test]
    fn the_compressed_copy_carries_the_pending_edits_and_records_no_step() {
        let mut model = open(&pdfs::text_document(&["one", "two", "three"]));
        model
            .apply_edit(PageEdit::Remove { index: 0, count: 1 })
            .unwrap();

        let copy = model
            .save_compressed(CompressPreset::Lossless, false)
            .unwrap();
        assert_eq!(page_count(&copy.bytes), 2);
        // Not an edit: the history is as it was, one step to undo.
        assert!(model.can_undo());
        assert!(!model.can_redo());
        assert!(model.undo());
        assert!(!model.can_undo());
        assert_eq!(model.page_count(), 3);
    }

    #[test]
    fn an_encrypted_document_is_refused_ahead_and_saved_as_is() {
        let model = DocumentModel::open(
            &pdfs::locked_document(&["one"]),
            &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
        )
        .expect("opens");
        let refusal = model.compression_refusal().expect("refused");
        assert_eq!(refusal.kind, "encryptedDocumentNotRewritable");
        assert!(refusal.message.contains("password"), "{}", refusal.message);

        let copy = model
            .save_compressed(CompressPreset::Small, false)
            .expect("the ordinary save, not an error");
        assert!(!copy.reduced);
        assert_eq!(copy.before_bytes, copy.after_bytes);
        assert_eq!(copy.refusals, [refusal]);
    }

    #[test]
    fn a_signed_document_needs_the_users_yes() {
        let model = open(pdfs::SIGNED);
        assert_eq!(model.compression_refusal(), None);
        assert!(model.compressed_save_will_invalidate_signatures().unwrap());

        let err = model
            .save_compressed(CompressPreset::Balanced, false)
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::SignaturesWouldBeInvalidated);

        let copy = model
            .save_compressed(CompressPreset::Balanced, true)
            .expect("acknowledged");
        assert!(copy.after_bytes <= copy.before_bytes);
        assert_eq!(page_count(&copy.bytes), 1);
    }

    #[test]
    fn an_unsigned_document_does_not_ask() {
        let model = open(&pdfs::text_document(&["one"]));
        assert!(!model.compressed_save_will_invalidate_signatures().unwrap());
    }
}
