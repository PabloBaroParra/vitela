//! Document properties — the text keys of the PDF's `/Info` dictionary —
//! over the core (B26 T-255).
//!
//! Every rule is the core's or the native shells': the value shown is the
//! last pending `Command::SetDocumentInfo`'s, else the file's own `/Info`
//! (`pdf_save::pending_document_info` over `LopdfDocument::document_info`, as
//! `pdf-ffi`'s `read_document_info` reads it); a change is one
//! `SetDocumentInfo` carrying the whole dictionary, so one undo step
//! restores the panel's prior state; `pdf-save` writes it into `/Info` at
//! save time. No XMP stream is touched, on any shell.
//!
//! What the native shells add, and this module keeps in Rust so no
//! TypeScript copy exists:
//!
//! - **The gate.** The core does not gate `SetDocumentInfo`
//!   (`EditClass::of` asks nothing of it); Linux, Windows and Android all ask
//!   the modify-contents permission (`content_editing_is_allowed`) before
//!   offering the properties, and so does this.
//! - **The six text keys.** Title, Author, Subject, Keywords, Creator and
//!   Producer are edited, as on Android; the two dates are not, and ride
//!   along from the current value into every change, as Android's adapter
//!   carries them.
//! - **An empty field is an absent key.** A text box cannot tell "cleared"
//!   from "never set", so `""` becomes `None` (the key is removed, never
//!   written empty) — Android's `metadataChange` rule.
//! - **Nothing new records nothing.** A change equal to the current value is
//!   not a step, so it does not light Undo or mark the document dirty.

use pdf_document::{Command, DocumentInfo};

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// Why a document whose permissions withhold modification cannot have its
/// properties changed — the Android sentence, as the core's lower-case
/// clause.
pub const METADATA_FORBIDDEN: &str = "this document does not permit metadata changes";

/// The text keys of `/Info` a properties dialog shows and edits. `None` is
/// an absent key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentProperties {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
}

impl DocumentProperties {
    fn of(info: &DocumentInfo) -> Self {
        DocumentProperties {
            title: info.title.clone(),
            author: info.author.clone(),
            subject: info.subject.clone(),
            keywords: info.keywords.clone(),
            creator: info.creator.clone(),
            producer: info.producer.clone(),
        }
    }

    /// `info` with these keys in place of its own; an empty one removed.
    fn over(self, info: &DocumentInfo) -> DocumentInfo {
        let key = |value: Option<String>| value.filter(|text| !text.is_empty());
        DocumentInfo {
            title: key(self.title),
            author: key(self.author),
            subject: key(self.subject),
            keywords: key(self.keywords),
            creator: key(self.creator),
            producer: key(self.producer),
            creation_date: info.creation_date,
            mod_date: info.mod_date,
        }
    }
}

impl DocumentModel {
    /// The whole `/Info` as the next save would write it.
    fn current_document_info(&self) -> DocumentInfo {
        pdf_save::pending_document_info(&self.document)
            .cloned()
            .unwrap_or_else(|| self.base.document_info())
    }

    /// The properties as the next save would write them: the last pending
    /// change's, else the file's. Read-only: answers however restricted the
    /// document is.
    pub fn document_properties(&self) -> DocumentProperties {
        DocumentProperties::of(&self.current_document_info())
    }

    /// Why this document's properties cannot be changed, or `None` when they
    /// can. Cheap: it reads the permissions.
    pub fn metadata_refusal(&self) -> Option<&'static str> {
        (!pdf_manip::content_editing_is_allowed(self.document.security.as_ref()))
            .then_some(METADATA_FORBIDDEN)
    }

    /// Records `properties` as one undo step (the dates kept, empty keys
    /// removed), or nothing when they are what the document already has.
    /// Refused with `EditNotPermitted` (see [`Self::metadata_refusal`]),
    /// recording nothing.
    pub fn set_document_properties(
        &mut self,
        properties: DocumentProperties,
    ) -> Result<(), EngineError> {
        if let Some(refusal) = self.metadata_refusal() {
            return Err(EngineError::new(ErrorKind::EditNotPermitted, refusal));
        }
        let before = self.current_document_info();
        let after = properties.over(&before);
        if after == before {
            return Ok(());
        }
        self.record(Command::SetDocumentInfo { before, after })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::pdfs;
    use pdf_document::PdfDate;
    use pdf_manip::OpenCredentials;

    fn open(bytes: &[u8]) -> DocumentModel {
        DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
    }

    fn text(value: &str) -> Option<String> {
        Some(value.to_string())
    }

    #[test]
    fn reads_the_files_own_keys_and_none_for_the_absent_ones() {
        let model = open(&pdfs::described_document());
        assert_eq!(
            model.document_properties(),
            DocumentProperties {
                title: text("Quarterly report"),
                author: text("Ana"),
                creator: text("Writer"),
                ..DocumentProperties::default()
            }
        );
        assert_eq!(model.metadata_refusal(), None);
        let bare = open(&pdfs::text_document(&["one"]));
        assert_eq!(bare.document_properties(), DocumentProperties::default());
    }

    #[test]
    fn a_change_is_one_undo_step_read_back_before_any_save() {
        let mut model = open(&pdfs::described_document());
        let mut edited = model.document_properties();
        edited.title = text("Annual report");
        edited.subject = text("Finance");
        model.set_document_properties(edited.clone()).unwrap();

        assert_eq!(model.document_properties(), edited);
        assert!(model.can_undo());
        assert!(model.undo());
        assert_eq!(model.document_properties().title, text("Quarterly report"));
        assert_eq!(model.document_properties().subject, None);
        assert!(model.redo());
        assert_eq!(model.document_properties(), edited);
    }

    #[test]
    fn the_save_writes_the_keys_and_keeps_the_files_creation_date() {
        let mut model = open(&pdfs::described_document());
        model
            .set_document_properties(DocumentProperties {
                title: text("Annual report"),
                author: text("Luis"),
                // Cleared in the dialog: the key goes.
                creator: text(""),
                ..DocumentProperties::default()
            })
            .unwrap();

        let saved = open(&model.save_to_bytes(false).unwrap());
        assert_eq!(
            saved.document_properties(),
            DocumentProperties {
                title: text("Annual report"),
                author: text("Luis"),
                ..DocumentProperties::default()
            }
        );
        assert_eq!(
            saved.current_document_info().creation_date,
            Some(PdfDate::parse(pdfs::DESCRIBED_CREATED).unwrap())
        );
    }

    #[test]
    fn what_the_document_already_has_records_nothing() {
        let mut model = open(&pdfs::described_document());
        let mut same = model.document_properties();
        model.set_document_properties(same.clone()).unwrap();
        // An empty field is the absent key it already is.
        same.subject = text("");
        model.set_document_properties(same).unwrap();
        assert!(!model.can_undo());
    }

    #[test]
    fn a_document_that_withholds_modification_shows_its_keys_but_refuses_a_change() {
        let mut model = DocumentModel::open(
            &pdfs::restricted_document(),
            &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
        assert_eq!(model.metadata_refusal(), Some(METADATA_FORBIDDEN));
        assert_eq!(model.document_properties(), DocumentProperties::default());

        let err = model
            .set_document_properties(DocumentProperties {
                title: text("Mine"),
                ..DocumentProperties::default()
            })
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::EditNotPermitted);
        assert_eq!(err.message, METADATA_FORBIDDEN);
        assert!(!model.can_undo());
    }

    #[test]
    fn a_change_rides_with_page_edits_in_one_history() {
        let mut model = open(&pdfs::text_document(&["one", "two"]));
        model
            .apply_edit(crate::model::PageEdit::Remove { index: 0, count: 1 })
            .unwrap();
        model
            .set_document_properties(DocumentProperties {
                title: text("Trimmed"),
                ..DocumentProperties::default()
            })
            .unwrap();
        assert!(model.undo());
        assert_eq!(model.document_properties().title, None);
        assert_eq!(model.page_count(), 1);
    }
}
