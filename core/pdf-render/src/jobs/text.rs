//! Text queries: one page's text runs, and a whole-document search.

use super::{document, page};
use crate::error::RenderError;
use crate::state::{DocHandle, PdfiumState};
use crate::text::{collect_text_runs, find_matches, TextMatch, TextRun};

pub(crate) fn text_runs_job(
    state: &mut PdfiumState,
    doc: DocHandle,
    page_index: u32,
) -> Result<Vec<TextRun>, RenderError> {
    let document = document(state, doc)?;
    let page = page(document, page_index)?;
    let text = page
        .text()
        .map_err(|e| RenderError::RenderFailed(e.to_string()))?;
    Ok(collect_text_runs(&text))
}

pub(crate) fn search_job(
    state: &mut PdfiumState,
    doc: DocHandle,
    query: &str,
) -> Result<Vec<TextMatch>, RenderError> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let document = document(state, doc)?;
    let count = document.pages().len() as u32;
    let mut matches = Vec::new();
    for page_index in 0..count {
        let page = page(document, page_index)?;
        let text = page
            .text()
            .map_err(|e| RenderError::RenderFailed(e.to_string()))?;
        matches.extend(find_matches(&collect_text_runs(&text), query, page_index));
    }
    Ok(matches)
}
