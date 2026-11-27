//! In-memory checkpoints for the web session (B26 T-258): the "save and
//! reopen" the core takes when a page-content edit needs one, mirroring
//! `pdf-ffi`'s `document/checkpoint.rs`.
//!
//! The policy — when a session may checkpoint, how a refused edit is rebound
//! to the new base, how undo and redo walk across one — is
//! `pdf_save::checkpoint`'s. What is left here is how this session hands its
//! parse, its bytes and its queue to it.

use pdf_document::Document;
use pdf_manip::LopdfDocument;

use super::DocumentModel;

/// One base as this session keeps it: the parse and the bytes it came from.
pub(super) type Base = (LopdfDocument, Vec<u8>);

impl pdf_save::CheckpointSession for DocumentModel {
    type Base = Base;

    fn document(&self) -> &Document {
        &self.document
    }

    fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    fn base(&self) -> &LopdfDocument {
        &self.base
    }

    /// Always `Some`: a web session is opened from bytes.
    fn original_bytes(&self) -> Option<&[u8]> {
        Some(&self.original_bytes)
    }

    fn has_password(&self) -> bool {
        self.has_password
    }

    fn has_imported_sources(&self) -> bool {
        !self.imported.is_empty()
    }

    fn checkpoints(&self) -> &pdf_save::Checkpoints<Base> {
        &self.checkpoints
    }

    fn checkpoints_mut(&mut self) -> &mut pdf_save::Checkpoints<Base> {
        &mut self.checkpoints
    }

    fn checkpointed_base(&self, base: LopdfDocument, bytes: Vec<u8>) -> Base {
        (base, bytes)
    }

    fn replace_base(&mut self, (base, bytes): Base) -> Base {
        let base = std::mem::replace(&mut self.base, base);
        let bytes = std::mem::replace(&mut self.original_bytes, bytes);
        (base, bytes)
    }
}

impl DocumentModel {
    /// Changes whenever the core writes the pending page-content edits into
    /// a new in-memory base, or undo or redo steps across one. Every content
    /// item read before the change is stale: re-read the page before
    /// building another content edit from it.
    pub fn content_base_generation(&self) -> u64 {
        self.checkpoints.generation()
    }

    /// Whether a page-content edit refused because its item already has an
    /// unsaved edit would instead be written into a new in-memory base and
    /// retried. `false` means the old rule holds: save and reopen first.
    pub fn content_edits_can_checkpoint(&self) -> bool {
        pdf_save::checkpoint::may_checkpoint(self)
    }
}
