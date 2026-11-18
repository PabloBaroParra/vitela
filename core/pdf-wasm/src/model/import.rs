//! Import ("Add PDFs") and the Documents view's blocks, over the core
//! (B26 T-227).
//!
//! Two phases, as in `pdf-ffi`: [`PreparedSources::prepare`] opens one picked
//! file (`pdf_save::prepare_source`: decryption, the source's copy
//! permission, the graft check) and keeps it under an id, so the shell can
//! ask a locked file for its password between files; then
//! [`DocumentModel::import_prepared`] adds every prepared source of a pick at
//! once (`pdf_save::import_sources`: the destination's gates, the position,
//! the id cursors, ONE `ImportPages`, registering the sources), so one undo
//! takes the whole pick back.
//!
//! What stays here is what only a boundary has: the ids JavaScript names the
//! prepared sources by (a JS value cannot hold a `LopdfDocument`), and the
//! "picked twice" check on those ids — the core takes each source as its own
//! `&mut` slot, so it can never be handed the same one twice (T-223).

use std::collections::HashMap;

use pdf_document::{derive_blocks, BlockSource};
use pdf_manip::LopdfDocument;

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// What [`PreparedSources::prepare`] reports about one prepared source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedInfo {
    /// The id [`DocumentModel::import_prepared`] takes it back by.
    pub id: u32,
    /// How many pages importing it adds — every page it has.
    pub page_count: u32,
    /// What its pages will not bring across exactly as they are.
    pub warnings: Vec<String>,
}

/// What [`DocumentModel::import_prepared`] added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    /// How many pages the whole pick added.
    pub page_count: u32,
    /// The block source id of each prepared source, in the order given — what
    /// [`BlockSpan::source`] names its pages by.
    pub source_ids: Vec<u32>,
}

/// The import sources prepared and not yet imported or discarded. One per
/// engine: a prepared source belongs to no document until it is imported.
#[derive(Default)]
pub struct PreparedSources {
    held: HashMap<u32, LopdfDocument>,
    next_id: u32,
}

impl PreparedSources {
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens `bytes` as an import source under `password` and keeps it.
    /// Refused with `PasswordRequired` or `WrongPassword` when it is
    /// encrypted and `password` does not open it (ask, then call again), and
    /// `UnsupportedOperation` when it forbids copying its pages, has none, or
    /// carries something an import cannot graft.
    pub fn prepare(
        &mut self,
        bytes: &[u8],
        password: Option<&str>,
    ) -> Result<PreparedInfo, EngineError> {
        let prepared = pdf_save::prepare_source(bytes, password)?;
        let id = self.next_id;
        // Ids are only compared for identity; one that wraps after four
        // billion prepares can only collide with a source long since spent.
        self.next_id = self.next_id.wrapping_add(1);
        let info = PreparedInfo {
            id,
            page_count: prepared.page_count(),
            warnings: prepared.warnings().to_vec(),
        };
        self.held.insert(id, prepared.into_source());
        Ok(info)
    }

    /// Drops a prepared source the shell no longer wants (a pick abandoned
    /// half-way). `false` when the id holds nothing.
    pub fn discard(&mut self, id: u32) -> bool {
        self.held.remove(&id).is_some()
    }

    /// How many sources are waiting.
    pub fn len(&self) -> usize {
        self.held.len()
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}

/// Which PDF a block's pages come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanSource {
    /// The PDF the document was opened from.
    Base,
    /// Inserted blank pages.
    Blank,
    /// A PDF added by an import; the id is one of its
    /// [`ImportReport::source_ids`].
    Imported(u32),
}

/// One block of the Documents view: a contiguous run of pages from one PDF,
/// by position — what the page edits (move, rotate, remove a run) take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockSpan {
    pub source: SpanSource,
    /// `Some(n)` (1-based) only when the source is split across more than
    /// one block.
    pub part: Option<u32>,
    /// Position of the block's first page now.
    pub start: u32,
    /// How many pages it holds — never zero.
    pub count: u32,
}

impl DocumentModel {
    /// Adds every page of the prepared sources `ids`, in that order, at
    /// position `index` (the page count appends), as ONE undoable edit.
    ///
    /// All or nothing: on any error the document is unchanged and every
    /// source is still prepared. Refused with `EditNotPermitted` when the
    /// document withholds a permission an import needs (asked first, as in
    /// `pdf-ffi`), `UnsupportedOperation` when its encryption cannot survive
    /// the rewrite, `ids` is empty, names one source twice, or names one that
    /// is not prepared (already imported, discarded, never prepared), and
    /// `PageIndexOutOfBounds` when `index` is past the end.
    pub fn import_prepared(
        &mut self,
        prepared: &mut PreparedSources,
        ids: &[u32],
        index: u32,
    ) -> Result<ImportReport, EngineError> {
        if let Some(refusal) = pdf_save::import_refusal(self.document.security.as_ref()) {
            return Err(pdf_save::ImportError::Refused(refusal).into());
        }
        for (position, id) in ids.iter().enumerate() {
            if ids[..position].contains(id) {
                return Err(EngineError::new(
                    ErrorKind::UnsupportedOperation,
                    "the same PDF was picked twice",
                ));
            }
        }

        // Each source leaves the map for the call and comes back unless the
        // import took it. One that is not held is an empty slot, which the
        // core refuses as already imported.
        let mut slots: Vec<Option<LopdfDocument>> =
            ids.iter().map(|id| prepared.held.remove(id)).collect();
        let outcome = {
            let mut borrowed: Vec<&mut Option<LopdfDocument>> = slots.iter_mut().collect();
            pdf_save::import_sources(
                &mut self.document,
                &mut self.imported,
                index as usize,
                &mut borrowed,
            )
        };
        for (id, slot) in ids.iter().zip(slots) {
            if let Some(source) = slot {
                prepared.held.insert(*id, source);
            }
        }
        let outcome = outcome?;

        let source_ids = outcome
            .ids
            .iter()
            .map(|id| source_id(id.0))
            .collect::<Result<_, _>>()?;
        Ok(ImportReport {
            page_count: outcome.page_count,
            source_ids,
        })
    }

    /// Whether the document would refuse any import right now, in the
    /// reader's words (`pdf_save::import_refusal`), or `None`. Asked before
    /// the file picker, so the reader is not sent to pick files that can only
    /// be refused.
    pub fn import_refusal(&self) -> Option<String> {
        pdf_save::import_refusal(self.document.security.as_ref()).map(|refusal| refusal.to_string())
    }

    /// Every block of the document in page order
    /// (`pdf_document::derive_blocks`), by position. Current until the next
    /// edit, like the page count.
    pub fn blocks(&self) -> Vec<BlockSpan> {
        let mut start = 0u32;
        derive_blocks(&self.document)
            .into_iter()
            .map(|block| {
                // The page count crosses this boundary as a `u32`, so no run
                // inside it can overflow one.
                let count = block.pages.len() as u32;
                let span = BlockSpan {
                    source: match block.source {
                        BlockSource::Base => SpanSource::Base,
                        BlockSource::Blank => SpanSource::Blank,
                        // Every imported id was checked into a `u32` when the
                        // import reported it.
                        BlockSource::Imported(id) => {
                            SpanSource::Imported(u32::try_from(id.0).unwrap_or(u32::MAX))
                        }
                    },
                    part: block.part,
                    start,
                    count,
                };
                start += count;
                span
            })
            .collect()
    }
}

/// An imported-document id as JavaScript names it. The session hands them
/// out one by one from 1, so running out of a `u32` takes four billion
/// imports; refuse rather than wrap.
fn source_id(id: u64) -> Result<u32, EngineError> {
    u32::try_from(id).map_err(|_| {
        EngineError::new(
            ErrorKind::UnsupportedOperation,
            "this document cannot import any more PDFs",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::pdfs;
    use crate::model::PageEdit;
    use pdf_manip::OpenCredentials;

    fn open(bytes: &[u8]) -> DocumentModel {
        DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
    }

    /// Each page's string, in page order, read back from saved bytes.
    fn page_texts(bytes: &[u8]) -> Vec<String> {
        let document = lopdf::Document::load_mem(bytes).expect("saved bytes parse");
        document
            .get_pages()
            .into_values()
            .map(|page| {
                let content = document.get_and_decode_page_content(page).expect("content");
                let shown = content
                    .operations
                    .iter()
                    .find(|operation| operation.operator == "Tj")
                    .expect("one Tj per page");
                String::from_utf8_lossy(shown.operands[0].as_str().expect("a literal string"))
                    .into_owned()
            })
            .collect()
    }

    fn sources(model: &DocumentModel) -> Vec<(SpanSource, u32, u32)> {
        model
            .blocks()
            .iter()
            .map(|block| (block.source, block.start, block.count))
            .collect()
    }

    #[test]
    fn a_pick_of_two_files_is_one_undo_step_and_saves_in_order() {
        let mut model = open(&pdfs::text_document(&["one", "two"]));
        let mut prepared = PreparedSources::new();
        let a = prepared
            .prepare(&pdfs::text_document(&["a1", "a2"]), None)
            .expect("prepares");
        let b = prepared
            .prepare(&pdfs::text_document(&["b1"]), None)
            .expect("prepares");
        assert_eq!((a.page_count, b.page_count), (2, 1));

        let report = model
            .import_prepared(&mut prepared, &[a.id, b.id], 1)
            .expect("imports");
        assert_eq!(report.page_count, 3);
        assert_eq!(report.source_ids.len(), 2);
        assert!(prepared.is_empty(), "the document took both sources");
        assert_eq!(
            page_texts(&model.save_to_bytes(false).unwrap()),
            ["one", "a1", "a2", "b1", "two"]
        );
        let [first, second] = [report.source_ids[0], report.source_ids[1]];
        assert_eq!(
            sources(&model),
            [
                (SpanSource::Base, 0, 1),
                (SpanSource::Imported(first), 1, 2),
                (SpanSource::Imported(second), 3, 1),
                (SpanSource::Base, 4, 1),
            ]
        );
        assert_eq!(model.blocks()[0].part, Some(1));
        assert_eq!(model.blocks()[3].part, Some(2));

        assert!(model.undo(), "one step takes the whole pick back");
        assert_eq!(model.page_count(), 2);
        assert!(!model.can_undo());
        assert!(model.redo());
        assert_eq!(
            page_texts(&model.preview_bytes().unwrap()),
            ["one", "a1", "a2", "b1", "two"],
            "the preview materializes imported pages"
        );
    }

    #[test]
    fn a_block_moves_as_a_run_of_pages() {
        let mut model = open(&pdfs::text_document(&["one", "two"]));
        let mut prepared = PreparedSources::new();
        let a = prepared
            .prepare(&pdfs::text_document(&["a1", "a2"]), None)
            .unwrap();
        model.import_prepared(&mut prepared, &[a.id], 2).unwrap();
        let imported = model.blocks()[1];
        model
            .apply_edit(PageEdit::Move {
                from: imported.start,
                count: imported.count,
                to: 0,
            })
            .unwrap();
        assert_eq!(
            page_texts(&model.save_to_bytes(false).unwrap()),
            ["a1", "a2", "one", "two"]
        );
    }

    #[test]
    fn an_encrypted_source_needs_its_own_password() {
        let mut prepared = PreparedSources::new();
        let locked = pdfs::locked_document(&["secret"]);
        let err = prepared.prepare(&locked, None).unwrap_err();
        assert!(
            matches!(
                err.kind,
                ErrorKind::PasswordRequired | ErrorKind::WrongPassword
            ),
            "{err:?}"
        );
        let err = prepared.prepare(&locked, Some("nope")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::WrongPassword);
        assert!(prepared.is_empty(), "a refused source is not kept");

        let ready = prepared
            .prepare(&locked, Some(pdfs::USER_PASSWORD))
            .expect("the password opens it");
        let mut model = open(&pdfs::text_document(&["one"]));
        model
            .import_prepared(&mut prepared, &[ready.id], 1)
            .unwrap();
        assert_eq!(
            page_texts(&model.save_to_bytes(false).unwrap()),
            ["one", "secret"]
        );
    }

    #[test]
    fn a_source_that_forbids_copying_is_unsupported() {
        let err = PreparedSources::new()
            .prepare(&pdfs::restricted_document(), Some(pdfs::USER_PASSWORD))
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsupportedOperation);
    }

    #[test]
    fn a_refused_import_keeps_every_source_prepared() {
        let mut model = open(&pdfs::text_document(&["one"]));
        let mut prepared = PreparedSources::new();
        let a = prepared
            .prepare(&pdfs::text_document(&["a"]), None)
            .unwrap();

        let err = model
            .import_prepared(&mut prepared, &[a.id], 5)
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::PageIndexOutOfBounds);
        let err = model
            .import_prepared(&mut prepared, &[a.id, a.id], 0)
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsupportedOperation);
        assert_eq!(err.message, "the same PDF was picked twice");
        let err = model.import_prepared(&mut prepared, &[], 0).unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsupportedOperation);
        assert_eq!(prepared.len(), 1);
        assert!(!model.can_undo());

        model.import_prepared(&mut prepared, &[a.id], 1).unwrap();
        let err = model
            .import_prepared(&mut prepared, &[a.id], 0)
            .unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::UnsupportedOperation,
            "a spent source is gone"
        );
    }

    #[test]
    fn a_discarded_source_cannot_be_imported() {
        let mut prepared = PreparedSources::new();
        let a = prepared
            .prepare(&pdfs::text_document(&["a"]), None)
            .unwrap();
        assert!(prepared.discard(a.id));
        assert!(!prepared.discard(a.id));
        let mut model = open(&pdfs::text_document(&["one"]));
        let err = model
            .import_prepared(&mut prepared, &[a.id], 0)
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsupportedOperation);
    }

    #[test]
    fn a_document_without_the_assembly_permission_refuses_an_import_first() {
        let mut model = DocumentModel::open(
            &pdfs::restricted_document(),
            &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
        assert!(model.import_refusal().is_some());
        let mut prepared = PreparedSources::new();
        let a = prepared
            .prepare(&pdfs::text_document(&["a"]), None)
            .unwrap();
        // Asked before the position: a refusing document says so even when
        // the position is also wrong.
        let err = model
            .import_prepared(&mut prepared, &[a.id], 9)
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::EditNotPermitted);
        assert_eq!(prepared.len(), 1);
    }

    #[test]
    fn an_unrestricted_document_refuses_no_import() {
        let model = open(&pdfs::text_document(&["one"]));
        assert_eq!(model.import_refusal(), None);
        assert_eq!(sources(&model), [(SpanSource::Base, 0, 1)]);
        assert_eq!(model.blocks()[0].part, None);
    }
}
