//! Lazy `PageContent` loading and the point-in-run hit-test behind a
//! content-edit-mode click.
//!
//! `read_page_content` is pure computation over the base document already
//! held in memory (no pdfium round trip, unlike `PageCharacters`), so it is
//! loaded synchronously the first time a page needs it and cached from then
//! on — no async/`_requested` bookkeeping to mirror. Pure functions over
//! `pdf-document`/`pdf-edit` values only, no GTK — same posture as
//! `annotations::geometry`.

use pdf_document::{PageContent, PageId, TextRun};
use pdf_edit::EditError;

/// Returns the content cached in `cache`, parsing `page_index` from `base`
/// on first use. Re-reports the same error on every call for a page whose
/// content stream this build cannot handle — errors are never cached as "no
/// content", so a transient failure does not haunt every later call.
pub(crate) fn ensure_page_content<'a>(
    cache: &'a mut Option<PageContent>,
    base: &lopdf::Document,
    page_index: usize,
) -> Result<&'a PageContent, EditError> {
    if cache.is_none() {
        *cache = Some(pdf_edit::read_page_content(
            base,
            PageId(page_index as u32),
        )?);
    }
    Ok(cache.as_ref().expect("just populated above"))
}

/// The text run whose bounding box contains `point` (in PDF page space),
/// or `None` if it lands outside every run on the page.
///
/// Ties broken by smallest bounding-box area: content-edit mode is about
/// picking out one run to retype, and the smaller of two overlapping runs is
/// the more specific — and therefore more likely intended — target.
pub(crate) fn text_run_at(content: &PageContent, point: (f32, f32)) -> Option<&TextRun> {
    content
        .text_runs
        .iter()
        .filter(|run| rect_contains(run, point))
        .min_by(|a, b| bbox_area(a).partial_cmp(&bbox_area(b)).unwrap())
}

fn rect_contains(run: &TextRun, (x, y): (f32, f32)) -> bool {
    let rect = &run.bbox;
    let x = f64::from(x);
    let y = f64::from(y);
    x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
}

fn bbox_area(run: &TextRun) -> f64 {
    run.bbox.width * run.bbox.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{annotation::Rect, ContentItemId, FontKind};

    fn run(id: u64, x: f64, y: f64, width: f64, height: f64) -> TextRun {
        TextRun {
            id: ContentItemId(id),
            page: PageId(0),
            bbox: Rect {
                x,
                y,
                width,
                height,
            },
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::Standard14,
            text: "Hello".to_string(),
        }
    }

    #[test]
    fn a_click_inside_a_runs_bbox_finds_it() {
        let content = PageContent {
            text_runs: vec![run(1, 100.0, 700.0, 50.0, 12.0)],
            images: Vec::new(),
        };

        let found = text_run_at(&content, (110.0, 705.0)).expect("point is inside the bbox");
        assert_eq!(found.id, ContentItemId(1));
    }

    #[test]
    fn a_click_outside_every_bbox_finds_nothing() {
        let content = PageContent {
            text_runs: vec![run(1, 100.0, 700.0, 50.0, 12.0)],
            images: Vec::new(),
        };

        assert!(text_run_at(&content, (0.0, 0.0)).is_none());
    }

    /// Two runs overlap (a rare but legal layout — e.g. text painted twice
    /// for a highlight effect). The smaller one is the more specific target.
    #[test]
    fn overlapping_runs_the_smaller_bbox_wins() {
        let content = PageContent {
            text_runs: vec![
                run(1, 0.0, 0.0, 200.0, 200.0),
                run(2, 50.0, 50.0, 20.0, 20.0),
            ],
            images: Vec::new(),
        };

        let found = text_run_at(&content, (55.0, 55.0)).expect("point is inside both bboxes");
        assert_eq!(found.id, ContentItemId(2));
    }

    #[test]
    fn ensure_page_content_reads_a_real_document_once_and_caches_it() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
        let mut cache = None;

        let first = ensure_page_content(&mut cache, &base, 0).expect("page 0 exists");
        assert_eq!(first.text_runs.len(), 1);
        assert_eq!(first.text_runs[0].text, "Hello world");
        assert!(cache.is_some());

        // A second call must not re-parse. We cannot observe "did not
        // re-parse" directly, but it must at least keep returning the same
        // snapshot from the now-populated cache.
        let second = ensure_page_content(&mut cache, &base, 0).expect("still page 0");
        assert_eq!(second.text_runs[0].text, "Hello world");
    }

    #[test]
    fn ensure_page_content_reports_a_missing_page_without_caching_the_failure() {
        let base = gen_fixtures::build_multi_line_page_document(&["Hello world"]);
        let mut cache = None;

        let error = ensure_page_content(&mut cache, &base, 7).expect_err("page 7 does not exist");
        assert_eq!(error, EditError::PageNotFound(PageId(7)));
        assert!(cache.is_none());
    }
}
