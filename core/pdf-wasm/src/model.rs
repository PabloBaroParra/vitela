//! The lopdf half of an open document: the model, its security policy, and
//! the page sizes a viewer lays out.
//!
//! None of this touches pdfium, so it builds and is tested on every target.
//! The rules themselves live below this crate — opening with credentials in
//! `pdf_manip::open`, page sizes in `pdf_manip::current_page_dimensions`, the
//! text gate in `pdf_manip::ensure_text_extraction_allowed` — exactly as
//! `pdf-ffi` uses them, so the web and the native shells cannot drift.

use pdf_document::{Document, Rotation};
use pdf_manip::{LopdfDocument, OpenCredentials, OpenedRotations};
use pdf_save::ImportedSourceRegistry;

use crate::error::{EngineError, ErrorKind};

/// One page's layout size in PDF points and the clockwise turn it is drawn
/// at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width_pt: f64,
    pub height_pt: f64,
    pub rotation_degrees: u32,
}

/// The model side of one open document, and its edit session (see
/// [`edit`]).
pub struct DocumentModel {
    document: Document,
    base: LopdfDocument,
    /// The bytes `base` was parsed from: every save and preview is computed
    /// from them plus the queued edits, as in `pdf-ffi`.
    original_bytes: Vec<u8>,
    /// The PDFs this session imported pages from, which a save resolves
    /// imported pages through. Empty until import lands (T-227), but held
    /// now so every save goes through the same registry `pdf-ffi` uses.
    imported: ImportedSourceRegistry,
    opened_rotations: OpenedRotations,
}

impl DocumentModel {
    /// Opens `bytes` under `credentials` and builds the editing model, the
    /// same two steps `pdf-ffi`'s `open_with_credentials` takes before it
    /// opens the renderer.
    pub fn open(bytes: &[u8], credentials: &OpenCredentials) -> Result<Self, EngineError> {
        let (base, security) =
            pdf_manip::open_document_with_credentials_from_bytes(bytes, credentials)?;
        // Building the model of a document lopdf has already parsed: a
        // failure here means the bytes are not a PDF this core can edit.
        let document = pdf_save::document_from_lopdf(&base, security)
            .map_err(|err| EngineError::new(ErrorKind::InvalidDocument, err.to_string()))?;
        // Read before any edit exists: at open every page sits at its own
        // base index.
        let opened_rotations = OpenedRotations::capture(&document);
        Ok(DocumentModel {
            document,
            base,
            original_bytes: bytes.to_vec(),
            imported: ImportedSourceRegistry::new(),
            opened_rotations,
        })
    }

    pub fn page_count(&self) -> u32 {
        self.document.pages.len() as u32
    }

    /// Every page's size, in the order the document has them now.
    pub fn page_sizes(&self) -> Vec<PageSize> {
        pdf_manip::current_page_dimensions(&self.document, &self.base, &self.opened_rotations)
            .into_iter()
            .map(|page| PageSize {
                width_pt: page.width_pt,
                height_pt: page.height_pt,
                rotation_degrees: degrees(page.rotation),
            })
            .collect()
    }

    /// Whether the document lets its text be copied or extracted.
    pub fn text_extraction_allowed(&self) -> bool {
        pdf_manip::text_extraction_is_allowed(self.document.security.as_ref())
    }

    /// The gate in front of every text read: text runs, page characters and
    /// search.
    pub fn ensure_text_extraction_allowed(&self) -> Result<(), EngineError> {
        pdf_manip::ensure_text_extraction_allowed(self.document.security.as_ref())?;
        Ok(())
    }

    /// Refuses a position past the last page.
    pub fn ensure_page(&self, page_index: u32) -> Result<(), EngineError> {
        if page_index < self.page_count() {
            Ok(())
        } else {
            Err(EngineError::page_out_of_bounds(page_index))
        }
    }
}

fn degrees(rotation: Rotation) -> u32 {
    match rotation {
        Rotation::None => 0,
        Rotation::Clockwise90 => 90,
        Rotation::Clockwise180 => 180,
        Rotation::Clockwise270 => 270,
    }
}

mod edit;

pub use edit::PageEdit;

// Declared at file level, not inside `mod tests`: there the path would be
// resolved through `src/model/tests/`, a directory that does not exist, which
// Linux and macOS refuse even when a later `..` leaves it again.
#[cfg(test)]
#[path = "../tests/support/pdfs.rs"]
mod pdfs;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn open(bytes: &[u8], credentials: OpenCredentials) -> Result<DocumentModel, EngineError> {
        DocumentModel::open(bytes, &credentials)
    }

    #[test]
    fn opens_a_plain_document_and_counts_its_pages() {
        let model = open(
            &pdfs::text_document(&["one", "two"]),
            OpenCredentials::NoPassword,
        )
        .expect("opens");
        assert_eq!(model.page_count(), 2);
        assert!(model.text_extraction_allowed());
    }

    #[test]
    fn reports_letter_size_upright() {
        let model = open(&pdfs::text_document(&["one"]), OpenCredentials::NoPassword).unwrap();
        assert_eq!(
            model.page_sizes(),
            vec![PageSize {
                width_pt: 612.0,
                height_pt: 792.0,
                rotation_degrees: 0,
            }]
        );
    }

    #[test]
    fn a_rotated_page_reports_its_turned_size_and_turn() {
        let model = open(&pdfs::rotated_document(90), OpenCredentials::NoPassword).unwrap();
        assert_eq!(
            model.page_sizes(),
            vec![PageSize {
                width_pt: 792.0,
                height_pt: 612.0,
                rotation_degrees: 90,
            }]
        );
    }

    #[test]
    fn an_encrypted_document_needs_a_password() {
        let bytes = pdfs::restricted_document();
        let err = open(&bytes, OpenCredentials::NoPassword).err().unwrap();
        assert_eq!(err.kind, ErrorKind::PasswordRequired);
        let err = open(&bytes, OpenCredentials::Password("nope".into()))
            .err()
            .unwrap();
        assert_eq!(err.kind, ErrorKind::WrongPassword);
    }

    #[test]
    fn the_user_password_honours_the_text_permission() {
        let model = open(
            &pdfs::restricted_document(),
            OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
        assert!(!model.text_extraction_allowed());
        assert_eq!(
            model.ensure_text_extraction_allowed().unwrap_err().kind,
            ErrorKind::TextExtractionDenied
        );
    }

    #[test]
    fn both_passwords_open_as_owner_and_lift_the_text_permission() {
        let model = open(
            &pdfs::restricted_document(),
            OpenCredentials::UserAndOwner {
                user: pdfs::USER_PASSWORD.into(),
                owner: pdfs::OWNER_PASSWORD.into(),
            },
        )
        .unwrap();
        assert!(model.text_extraction_allowed());
    }

    #[test]
    fn dual_open_refuses_a_wrong_owner_password() {
        let err = open(
            &pdfs::restricted_document(),
            OpenCredentials::UserAndOwner {
                user: pdfs::USER_PASSWORD.into(),
                owner: "nope".into(),
            },
        )
        .err()
        .unwrap();
        assert_eq!(err.kind, ErrorKind::WrongPassword);
    }

    #[test]
    fn a_promptless_open_still_honours_the_text_permission() {
        let model = open(
            &pdfs::promptless_restricted_document(),
            OpenCredentials::NoPassword,
        )
        .expect("an empty user password opens without a prompt");
        assert!(!model.text_extraction_allowed());
        assert_eq!(
            model.ensure_text_extraction_allowed().unwrap_err().kind,
            ErrorKind::TextExtractionDenied
        );
    }

    #[test]
    fn the_owner_password_lifts_a_promptless_restriction() {
        let model = open(
            &pdfs::promptless_restricted_document(),
            OpenCredentials::Password(pdfs::OWNER_PASSWORD.into()),
        )
        .unwrap();
        assert!(model.text_extraction_allowed());

        let err = open(
            &pdfs::promptless_restricted_document(),
            OpenCredentials::Password("nope".into()),
        )
        .err()
        .unwrap();
        assert_eq!(err.kind, ErrorKind::WrongPassword);
    }

    #[test]
    fn ensure_page_refuses_past_the_last_page() {
        let model = open(&pdfs::text_document(&["one"]), OpenCredentials::NoPassword).unwrap();
        assert!(model.ensure_page(0).is_ok());
        assert_eq!(
            model.ensure_page(1).unwrap_err().kind,
            ErrorKind::PageIndexOutOfBounds
        );
    }
}
