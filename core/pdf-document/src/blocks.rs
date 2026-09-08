//! Block derivation for the Organize "Documents" view (checklist §2,
//! `docs/batch-pdf-assembly.md`).
//!
//! A block is a maximal contiguous run of pages sharing the same PDF
//! source. Blocks are never stored on `Document` — they are derived fresh
//! from `Document.pages` on every call, so a caller never needs to
//! invalidate a cache after import, move, delete, undo or redo; it just
//! calls [`derive_blocks`] again.

use std::collections::HashMap;

use crate::document::{Document, ImportedDocumentId, PageId, PageOrigin};

/// Which PDF a block's pages come from — the identity blocks are grouped
/// by. Deliberately narrower than [`PageOrigin`]: it drops `page_index`,
/// since two pages from the same PDF at different source indices still
/// belong to the same block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockSource {
    /// The immutable PDF this session was opened with.
    Base,
    /// Pages created without source PDF content.
    Blank,
    /// A separately registered imported PDF.
    Imported(ImportedDocumentId),
}

impl From<PageOrigin> for BlockSource {
    fn from(origin: PageOrigin) -> Self {
        match origin {
            PageOrigin::Base { .. } => BlockSource::Base,
            PageOrigin::Blank => BlockSource::Blank,
            PageOrigin::Imported { source, .. } => BlockSource::Imported(source),
        }
    }
}

/// One contiguous run of pages sharing a [`BlockSource`].
///
/// Identified by `anchor`, the stable [`PageId`] of the run's first page —
/// never by its position in the `Vec` [`derive_blocks`] returns, which
/// shifts on every recompute. `part` is `Some(n)` (1-based) only when
/// `source` appears in more than one block, so a caller renders "Part 1",
/// "Part 2" without this model committing to that exact string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub anchor: PageId,
    pub source: BlockSource,
    pub pages: Vec<PageId>,
    pub part: Option<u32>,
}

/// Derives the Documents-view blocks from `document`'s current page order.
///
/// Walks `Document.pages` left to right and starts a new block whenever the
/// source differs from the previous page — including when a source seen
/// earlier reappears after an intervening one, since that is exactly what
/// makes a PDF's pages non-contiguous and worth splitting into labeled
/// parts (checklist: "Crear un bloque nuevo cuando reaparezca una fuente").
/// Read-only: takes `&Document`, so it cannot mutate it, and never caches —
/// every call is a fresh derivation from the page order it is given.
pub fn derive_blocks(document: &Document) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();

    for page in &document.pages {
        let source = BlockSource::from(page.origin);
        match blocks.last_mut() {
            Some(block) if block.source == source => block.pages.push(page.id),
            _ => blocks.push(Block {
                anchor: page.id,
                source,
                pages: vec![page.id],
                part: None,
            }),
        }
    }

    label_split_parts(&mut blocks);
    blocks
}

/// Assigns 1-based `part` numbers, in block order, to every block whose
/// source appears in more than one block. A source confined to a single
/// block keeps `part: None` — it was never split, so it has nothing to
/// number.
fn label_split_parts(blocks: &mut [Block]) {
    let mut occurrences: HashMap<BlockSource, u32> = HashMap::new();
    for block in blocks.iter() {
        *occurrences.entry(block.source).or_insert(0) += 1;
    }

    let mut seen: HashMap<BlockSource, u32> = HashMap::new();
    for block in blocks.iter_mut() {
        if occurrences[&block.source] > 1 {
            let next = seen.entry(block.source).or_insert(0);
            *next += 1;
            block.part = Some(*next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Orientation, Page, PageSize, Rotation};
    use crate::edit_log::{Command, EditLog};

    const SOURCE_A: ImportedDocumentId = ImportedDocumentId(1);
    const SOURCE_B: ImportedDocumentId = ImportedDocumentId(2);

    fn imported(id: u32, source: ImportedDocumentId, page_index: u32) -> Page {
        Page::imported(
            PageId(id),
            source,
            page_index,
            PageSize::A4,
            Orientation::Portrait,
            Rotation::None,
        )
    }

    fn base(id: u32, page_index: u32) -> Page {
        Page::base(
            PageId(id),
            page_index,
            PageSize::A4,
            Orientation::Portrait,
            Rotation::None,
        )
    }

    fn blank(id: u32) -> Page {
        Page::blank(PageId(id), PageSize::A4, Orientation::Portrait)
    }

    fn document_with(pages: Vec<Page>) -> Document {
        Document {
            pages,
            ..Document::default()
        }
    }

    #[test]
    fn a_single_source_document_is_one_block() {
        let document = document_with(vec![base(0, 0), base(1, 1), base(2, 2)]);

        let blocks = derive_blocks(&document);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].source, BlockSource::Base);
        assert_eq!(blocks[0].pages, vec![PageId(0), PageId(1), PageId(2)]);
        assert_eq!(blocks[0].part, None);
    }

    #[test]
    fn contiguous_pages_from_the_same_source_stay_in_one_block() {
        let document = document_with(vec![
            imported(0, SOURCE_A, 0),
            imported(1, SOURCE_A, 1),
            imported(2, SOURCE_A, 2),
        ]);

        let blocks = derive_blocks(&document);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].pages, vec![PageId(0), PageId(1), PageId(2)]);
    }

    #[test]
    fn a_source_change_starts_a_new_block() {
        let document = document_with(vec![imported(0, SOURCE_A, 0), imported(1, SOURCE_B, 0)]);

        let blocks = derive_blocks(&document);

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].source, BlockSource::Imported(SOURCE_A));
        assert_eq!(blocks[1].source, BlockSource::Imported(SOURCE_B));
    }

    #[test]
    fn a_reappearing_source_starts_a_new_block_rather_than_merging() {
        // A1, B1, A2 — A is not contiguous, so it must be two blocks, not one.
        let document = document_with(vec![
            imported(0, SOURCE_A, 0),
            imported(1, SOURCE_B, 0),
            imported(2, SOURCE_A, 1),
        ]);

        let blocks = derive_blocks(&document);

        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].source, BlockSource::Imported(SOURCE_A));
        assert_eq!(blocks[1].source, BlockSource::Imported(SOURCE_B));
        assert_eq!(blocks[2].source, BlockSource::Imported(SOURCE_A));
    }

    #[test]
    fn interleaved_sources_derive_the_expected_blocks_in_order() {
        // A1, A2, B1, A3, B2
        let document = document_with(vec![
            imported(0, SOURCE_A, 0),
            imported(1, SOURCE_A, 1),
            imported(2, SOURCE_B, 0),
            imported(3, SOURCE_A, 2),
            imported(4, SOURCE_B, 1),
        ]);

        let blocks = derive_blocks(&document);

        assert_eq!(
            blocks
                .iter()
                .map(|b| (b.source, b.pages.clone(), b.part))
                .collect::<Vec<_>>(),
            vec![
                (
                    BlockSource::Imported(SOURCE_A),
                    vec![PageId(0), PageId(1)],
                    Some(1)
                ),
                (BlockSource::Imported(SOURCE_B), vec![PageId(2)], Some(1)),
                (BlockSource::Imported(SOURCE_A), vec![PageId(3)], Some(2)),
                (BlockSource::Imported(SOURCE_B), vec![PageId(4)], Some(2)),
            ]
        );
    }

    #[test]
    fn a_source_confined_to_one_block_is_not_labeled_a_part() {
        let document = document_with(vec![imported(0, SOURCE_A, 0), imported(1, SOURCE_B, 0)]);

        let blocks = derive_blocks(&document);

        assert!(blocks.iter().all(|b| b.part.is_none()));
    }

    #[test]
    fn each_block_is_identified_by_its_first_pages_stable_id() {
        let document = document_with(vec![
            imported(9, SOURCE_A, 0),
            imported(4, SOURCE_A, 1),
            imported(7, SOURCE_B, 0),
        ]);

        let blocks = derive_blocks(&document);

        assert_eq!(blocks[0].anchor, PageId(9));
        assert_eq!(blocks[1].anchor, PageId(7));
    }

    #[test]
    fn deriving_blocks_never_mutates_the_document() {
        let document = document_with(vec![imported(0, SOURCE_A, 0), base(1, 0), blank(2)]);
        let untouched = document.clone();

        let _ = derive_blocks(&document);

        assert_eq!(document, untouched);
    }

    #[test]
    fn blocks_recompute_correctly_after_import_move_undo_and_redo() {
        let mut document = document_with(vec![base(0, 0)]);
        let mut log = EditLog::new();

        // Import a two-page batch from source A after the base page.
        log.apply(
            &mut document,
            Command::ImportPages {
                index: 1,
                pages: vec![imported(10, SOURCE_A, 0), imported(11, SOURCE_A, 1)],
            },
        );
        let after_import = derive_blocks(&document);
        assert_eq!(
            after_import.iter().map(|b| b.source).collect::<Vec<_>>(),
            vec![BlockSource::Base, BlockSource::Imported(SOURCE_A)]
        );

        // Move the base page to the end — it now trails the imported block.
        log.apply(
            &mut document,
            Command::MovePages {
                from: 0,
                count: 1,
                to: 2,
            },
        );
        let after_move = derive_blocks(&document);
        assert_eq!(
            after_move.iter().map(|b| b.source).collect::<Vec<_>>(),
            vec![BlockSource::Imported(SOURCE_A), BlockSource::Base]
        );

        // Undo the move: back to base-then-import.
        log.undo(&mut document);
        assert_eq!(derive_blocks(&document), after_import);

        // Redo it: back to import-then-base.
        log.redo(&mut document);
        assert_eq!(derive_blocks(&document), after_move);
    }
}
