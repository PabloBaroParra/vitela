//! In-memory checkpoints: the "save and reopen" a reader used to do by hand,
//! done by the core when an edit needs it.
//!
//! The policy — when a session may checkpoint, how a refused edit is rebound
//! to the new base, how undo and redo walk across one — is
//! `pdf_save::checkpoint`'s, shared with the shells that call the core
//! directly. What is left here is how this session hands its parse, its bytes
//! and its queue to it.

use super::*;

/// One base as this session keeps it: the parse and the bytes it came from.
pub(super) type Base = (LopdfDocument, Vec<u8>);

impl pdf_save::CheckpointSession for DocumentState {
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

    fn original_bytes(&self) -> Option<&[u8]> {
        self.original_bytes.as_deref()
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
        let bytes = self
            .original_bytes
            .replace(bytes)
            .expect("a checkpointed session has original bytes");
        (base, bytes)
    }
}

impl DocumentState {
    /// Records `command` through `apply`, checkpointing once when `apply`
    /// refuses it and the session may take one. See
    /// `pdf_save::checkpoint::record`.
    pub(crate) fn record_with_checkpoint(
        &mut self,
        command: Command,
        apply: fn(&mut DocumentState, Command) -> Result<(), FfiError>,
    ) -> Result<(), FfiError> {
        pdf_save::checkpoint::record(self, command, apply)
    }

    pub(super) fn can_undo(&self) -> bool {
        pdf_save::checkpoint::can_undo(self)
    }

    pub(super) fn can_redo(&self) -> bool {
        pdf_save::checkpoint::can_redo(self)
    }

    pub(super) fn content_base_generation(&self) -> u64 {
        self.checkpoints.generation()
    }

    pub(super) fn undo(&mut self) -> bool {
        pdf_save::checkpoint::undo(self)
    }

    pub(super) fn redo(&mut self) -> bool {
        pdf_save::checkpoint::redo(self)
    }

    pub(super) fn may_checkpoint(&self) -> bool {
        pdf_save::checkpoint::may_checkpoint(self)
    }
}
