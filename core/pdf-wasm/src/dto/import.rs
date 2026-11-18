//! The values `Engine`'s import surface and the Documents view hand to
//! JavaScript (B26 T-227).

use wasm_bindgen::prelude::*;

use crate::model::{BlockSpan, ImportReport, PreparedInfo, SpanSource};

/// One import source opened by `Engine.prepareImport`, waiting for
/// `importPrepared`.
#[wasm_bindgen]
#[derive(Debug)]
pub struct PreparedImport {
    id: u32,
    page_count: u32,
    warnings: Vec<String>,
}

impl From<PreparedInfo> for PreparedImport {
    fn from(info: PreparedInfo) -> Self {
        PreparedImport {
            id: info.id,
            page_count: info.page_count,
            warnings: info.warnings,
        }
    }
}

#[wasm_bindgen]
impl PreparedImport {
    /// What `importPrepared` and `discardPrepared` name it by.
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> u32 {
        self.id
    }

    /// How many pages importing it adds — every page it has.
    #[wasm_bindgen(getter, js_name = pageCount)]
    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    /// What its pages will not bring across exactly as they are, one
    /// reader-facing line each.
    #[wasm_bindgen(getter)]
    pub fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }
}

/// What `Engine.importPrepared` added.
#[wasm_bindgen]
#[derive(Debug)]
pub struct ImportedPages {
    page_count: u32,
    source_ids: Vec<u32>,
}

impl From<ImportReport> for ImportedPages {
    fn from(report: ImportReport) -> Self {
        ImportedPages {
            page_count: report.page_count,
            source_ids: report.source_ids,
        }
    }
}

#[wasm_bindgen]
impl ImportedPages {
    /// How many pages the whole pick added.
    #[wasm_bindgen(getter, js_name = pageCount)]
    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    /// The block source id of each prepared source, in the order given —
    /// what `DocumentBlock.sourceId` names its pages by.
    #[wasm_bindgen(getter, js_name = sourceIds)]
    pub fn source_ids(&self) -> Vec<u32> {
        self.source_ids.clone()
    }
}

/// One block of the Documents view: a contiguous run of pages from one PDF,
/// by position.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct DocumentBlock {
    span: BlockSpan,
}

impl From<BlockSpan> for DocumentBlock {
    fn from(span: BlockSpan) -> Self {
        DocumentBlock { span }
    }
}

#[wasm_bindgen]
impl DocumentBlock {
    /// `"base"` (the opened PDF), `"blank"` (inserted blank pages) or
    /// `"imported"` (see `sourceId`).
    #[wasm_bindgen(getter)]
    pub fn source(&self) -> String {
        match self.span.source {
            SpanSource::Base => "base",
            SpanSource::Blank => "blank",
            SpanSource::Imported(_) => "imported",
        }
        .to_string()
    }

    /// For an imported block, the id `importPrepared` reported for its PDF.
    #[wasm_bindgen(getter, js_name = sourceId)]
    pub fn source_id(&self) -> Option<u32> {
        match self.span.source {
            SpanSource::Imported(id) => Some(id),
            SpanSource::Base | SpanSource::Blank => None,
        }
    }

    /// 1-based, only when the block's PDF is split across several blocks.
    #[wasm_bindgen(getter)]
    pub fn part(&self) -> Option<u32> {
        self.span.part
    }

    /// Position of the block's first page now.
    #[wasm_bindgen(getter)]
    pub fn start(&self) -> u32 {
        self.span.start
    }

    /// How many pages the block holds — never zero.
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> u32 {
        self.span.count
    }
}
