//! The Organize screen's Documents view, across the UniFFI boundary: the
//! open document's pages grouped into one block per contiguous run from the
//! same PDF.
//!
//! The grouping is `pdf_document::derive_blocks`, the same derivation the
//! Linux shell's `organize::documents` view reads, so a shell on this side of
//! the boundary cannot disagree with it about where one document ends.
//!
//! ## Positions, not page ids
//!
//! The core identifies a block by its first page's `PageId`. This boundary
//! never hands a `PageId` out (see `FfiEditCommand`'s own docs), so a block
//! is reported by `start` and `count` instead — exactly what the block
//! commands (`MovePages`, `RemovePages`, `RotatePages`) take back. The
//! positions are only current until the next edit; a shell asks again after
//! every one, the same way it re-reads the page count.
//!
//! ## Names stay in the shell
//!
//! An imported block carries the id its `import_pdf` call reported
//! (`FfiImportReport::source_id`), not a file name: the bytes a shell hands
//! over have no name, and the shell already knows which file it picked.

use pdf_document::{derive_blocks, BlockSource};

use crate::document::DocumentHandle;

/// Which PDF a block's pages come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiBlockSource {
    /// The PDF the handle was opened from.
    Base,
    /// Pages with no source PDF at all — inserted blank pages.
    Blank,
    /// A PDF added by `import_pdf`; `id` is that call's
    /// `FfiImportReport::source_id`.
    Imported { id: u64 },
}

impl From<BlockSource> for FfiBlockSource {
    fn from(source: BlockSource) -> Self {
        match source {
            BlockSource::Base => FfiBlockSource::Base,
            BlockSource::Blank => FfiBlockSource::Blank,
            BlockSource::Imported(id) => FfiBlockSource::Imported { id: id.0 },
        }
    }
}

/// One contiguous run of pages from the same source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct FfiDocumentBlock {
    pub source: FfiBlockSource,
    /// `Some(n)` (1-based) only when `source` is split across more than one
    /// block, so a shell can label "Part 1", "Part 2"; `None` otherwise.
    pub part: Option<u32>,
    /// Position of the block's first page in the current page order.
    pub start: u32,
    /// How many pages the block holds — never zero.
    pub count: u32,
}

/// Every block of the open document, in page order.
#[uniffi::export]
pub fn document_blocks(handle: &DocumentHandle) -> Vec<FfiDocumentBlock> {
    let state = handle.lock();
    let mut start = 0u32;
    derive_blocks(state.document())
        .into_iter()
        .map(|block| {
            // A document's page count already crosses this boundary as a
            // `u32` (`page_count`), so no run inside it can overflow one.
            let count = block.pages.len() as u32;
            let reported = FfiDocumentBlock {
                source: block.source.into(),
                part: block.part,
                start,
                count,
            };
            start += count;
            reported
        })
        .collect()
}
