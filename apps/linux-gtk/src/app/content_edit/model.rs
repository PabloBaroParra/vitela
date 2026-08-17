//! Lazy `PageContent` loading and the point-in-run hit-test behind a
//! content-edit-mode click.
//!
//! `read_page_content` is pure computation over the base document already
//! held in memory (no pdfium round trip, unlike `PageCharacters`), so it is
//! loaded synchronously the first time a page needs it and cached from then
//! on — no async/`_requested` bookkeeping to mirror. Pure functions over
//! `pdf-document`/`pdf-edit` values only, no GTK — same posture as
//! `annotations::geometry`.

use pdf_document::{Command, EditLog, ImageItem, PageContent, PageId, Rect, TextRun};
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
        .filter(|run| rect_contains(&run.bbox, point))
        .min_by(|a, b| bbox_area(&a.bbox).partial_cmp(&bbox_area(&b.bbox)).unwrap())
}

/// The image whose bounding box contains `point` (in PDF page space), or
/// `None` if it lands outside every image on the page. Sibling of
/// [`text_run_at`], same smallest-bbox tie-break: content-edit mode is about
/// picking out one item to act on, and the smaller of two overlapping items
/// is the more specific — and therefore more likely intended — target.
pub(crate) fn image_at(content: &PageContent, point: (f32, f32)) -> Option<&ImageItem> {
    content
        .images
        .iter()
        .filter(|image| rect_contains(&image.bbox, point))
        .min_by(|a, b| bbox_area(&a.bbox).partial_cmp(&bbox_area(&b.bbox)).unwrap())
}

/// Picks a font resource name not already used by any text run currently
/// parsed on the page (T-163's "insert text" sub-mode).
///
/// This matters more than a cosmetic naming choice:
/// `pdf_edit::insert::ensure_font_resource` *reuses* whatever resource
/// dictionary already answers to a given name rather than creating a new
/// one — that is exactly what lets replaying an insertion twice, or two
/// insertions sharing one font, avoid piling up duplicate font objects. But
/// it also means a name that happens to already name some *other* font would
/// silently register the new run against the wrong glyph set instead of
/// failing loudly, so the caller has to pick a genuinely unused name up
/// front rather than lean on `pdf-edit` to notice a collision — it cannot.
///
/// Only checks the runs `PageContent` actually exposes, not the page's full
/// `/Resources /Font` dictionary: `PageContent` is a snapshot of what the
/// content stream *paints*, and does not carry a resource entry no run
/// references. A font registered but never painted by any run is a rare,
/// pre-existing oddity outside this model's visibility; the `FIns` prefix —
/// distinct from the `F1`, `F2`, … a producer typically assigns — further
/// narrows the (already narrow) chance of landing on exactly such an entry.
///
/// `reserved` closes the gap `PageContent` alone cannot: it is parsed from
/// the *base* document, which never carries edits still sitting in the
/// `EditLog`, so a second insertion made before a save would otherwise pick
/// the name the first one already claimed. Pass
/// [`reserved_font_resource_names`]'s result.
pub(crate) fn unused_font_resource_name(content: &PageContent, reserved: &[String]) -> String {
    let mut candidate_number = 1u32;
    loop {
        let candidate = format!("FIns{candidate_number}");
        let taken = content
            .text_runs
            .iter()
            .any(|run| run.resource_font_name == candidate)
            || reserved.contains(&candidate);
        if !taken {
            return candidate;
        }
        candidate_number += 1;
    }
}

/// The image twin of [`unused_font_resource_name`] — same reasoning, same
/// caveat about only seeing resources `PageContent` actually paints, applied
/// to `/Resources /XObject` instead of `/Resources /Font` (T-163's "insert
/// image" sub-mode).
///
/// `reserved` matters more here than it does for fonts. A duplicate font
/// name degrades quietly (`ensure_font_resource` reuses the entry, and two
/// Standard-14 runs sharing one resource render correctly); a duplicate
/// XObject name is refused outright by `pdf_edit::insert_image`
/// (`EditError::ResourceNameInUse`), and since that refusal happens during
/// `replay_content_edits` it takes the entire save down with it.
pub(crate) fn unused_xobject_resource_name(content: &PageContent, reserved: &[String]) -> String {
    let mut candidate_number = 1u32;
    loop {
        let candidate = format!("XIns{candidate_number}");
        let taken = content
            .images
            .iter()
            .any(|image| image.resource_xobject_name == candidate)
            || reserved.contains(&candidate);
        if !taken {
            return candidate;
        }
        candidate_number += 1;
    }
}

/// The font resource names already claimed by insertions queued in `pending`
/// but not yet folded into any base document.
///
/// Deliberately not filtered by page: a `/Resources` dictionary can be
/// inherited from the page tree or shared outright between pages, which is
/// the same reason `pdf_edit::insert_image`'s own doc warns that registering
/// a name affects "every other page sharing the dictionary". Treating a name
/// claimed on one page as unavailable everywhere over-reserves in the
/// unshared case — which costs nothing but a higher suffix.
pub(crate) fn reserved_font_resource_names(pending: &EditLog) -> Vec<String> {
    pending
        .entries()
        .iter()
        .filter_map(|command| match command {
            Command::InsertTextRun(run) => Some(run.resource_font_name.clone()),
            _ => None,
        })
        .collect()
}

/// The XObject twin of [`reserved_font_resource_names`].
///
/// Covers `RemoveImage` as well as `InsertImage`: removing an image takes
/// the paint operation off the page but deliberately leaves its XObject
/// registered (so undo can put the picture back without carrying the bytes),
/// which means the name stays occupied even though nothing paints it any
/// more — exactly the case `PageContent` cannot see.
pub(crate) fn reserved_xobject_resource_names(pending: &EditLog) -> Vec<String> {
    pending
        .entries()
        .iter()
        .filter_map(|command| match command {
            Command::InsertImage { item, .. } | Command::RemoveImage { item, .. } => {
                Some(item.resource_xobject_name.clone())
            }
            _ => None,
        })
        .collect()
}

fn rect_contains(rect: &Rect, (x, y): (f32, f32)) -> bool {
    let x = f64::from(x);
    let y = f64::from(y);
    x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
}

fn bbox_area(rect: &Rect) -> f64 {
    rect.width * rect.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{annotation::Rect, ContentItemId, FontKind};

    fn image(id: u64, x: f64, y: f64, width: f64, height: f64) -> ImageItem {
        ImageItem {
            id: ContentItemId(id),
            page: PageId(0),
            bbox: Rect {
                x,
                y,
                width,
                height,
            },
            resource_xobject_name: "Im1".to_string(),
        }
    }

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
    fn a_click_inside_an_images_bbox_finds_it() {
        let content = PageContent {
            text_runs: Vec::new(),
            images: vec![image(1, 100.0, 600.0, 80.0, 40.0)],
        };

        let found = image_at(&content, (140.0, 620.0)).expect("point is inside the bbox");
        assert_eq!(found.id, ContentItemId(1));
    }

    #[test]
    fn a_click_outside_every_images_bbox_finds_nothing() {
        let content = PageContent {
            text_runs: Vec::new(),
            images: vec![image(1, 100.0, 600.0, 80.0, 40.0)],
        };

        assert!(image_at(&content, (0.0, 0.0)).is_none());
    }

    /// Two images overlap (e.g. a placeholder painted under a real photo).
    /// The smaller one is the more specific target.
    #[test]
    fn overlapping_images_the_smaller_bbox_wins() {
        let content = PageContent {
            text_runs: Vec::new(),
            images: vec![
                image(1, 0.0, 0.0, 200.0, 200.0),
                image(2, 50.0, 50.0, 20.0, 20.0),
            ],
        };

        let found = image_at(&content, (55.0, 55.0)).expect("point is inside both bboxes");
        assert_eq!(found.id, ContentItemId(2));
    }

    #[test]
    fn image_at_reads_a_real_fixture_document_and_finds_the_right_image() {
        let base = gen_fixtures::content_edit::build_roundtrip_image_page_document();
        let content = pdf_edit::read_page_content(&base, PageId(0)).expect("page 0 parses");

        let found = image_at(&content, (110.0, 610.0)).expect("point is inside the target image");
        assert_eq!(
            found.resource_xobject_name,
            gen_fixtures::content_edit::TARGET_IMAGE_RESOURCE_NAME
        );

        // The control image, painted elsewhere on the page, must not be hit
        // by a click aimed at the target.
        assert_ne!(
            found.resource_xobject_name,
            gen_fixtures::content_edit::CONTROL_IMAGE_RESOURCE_NAME
        );
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

    // --- unused_font_resource_name / unused_xobject_resource_name (T-163) -

    fn empty_page() -> PageContent {
        PageContent {
            text_runs: Vec::new(),
            images: Vec::new(),
        }
    }

    #[test]
    fn an_empty_page_gets_the_first_candidate_name() {
        let content = empty_page();

        assert_eq!(unused_font_resource_name(&content, &[]), "FIns1");
        assert_eq!(unused_xobject_resource_name(&content, &[]), "XIns1");
    }

    #[test]
    fn a_taken_font_name_is_skipped_for_the_next_candidate() {
        let mut taken = run(1, 0.0, 0.0, 10.0, 10.0);
        taken.resource_font_name = "FIns1".to_string();
        let content = PageContent {
            text_runs: vec![taken],
            images: Vec::new(),
        };

        assert_eq!(unused_font_resource_name(&content, &[]), "FIns2");
    }

    #[test]
    fn a_taken_xobject_name_is_skipped_for_the_next_candidate() {
        let mut taken = image(1, 0.0, 0.0, 10.0, 10.0);
        taken.resource_xobject_name = "XIns1".to_string();
        let content = PageContent {
            text_runs: Vec::new(),
            images: vec![taken],
        };

        assert_eq!(unused_xobject_resource_name(&content, &[]), "XIns2");
    }

    /// Font names and XObject names are independent namespaces in
    /// `/Resources`, so a name taken in one must not influence the other.
    #[test]
    fn font_and_xobject_naming_do_not_interfere_with_each_other() {
        let mut taken_font = run(1, 0.0, 0.0, 10.0, 10.0);
        taken_font.resource_font_name = "FIns1".to_string();
        let content = PageContent {
            text_runs: vec![taken_font],
            images: Vec::new(),
        };

        assert_eq!(unused_xobject_resource_name(&content, &[]), "XIns1");
    }

    // --- reserved_*_resource_names (queued-but-unsaved insertions) --------

    /// The case the base document cannot answer on its own: a second
    /// insertion made before any save must not reuse the name the first one
    /// already claimed, even though `PageContent` — parsed from bytes that
    /// predate both — still shows the page as empty.
    #[test]
    fn a_name_claimed_by_a_queued_insertion_is_skipped() {
        let content = empty_page();
        let reserved = vec!["XIns1".to_string()];

        assert_eq!(unused_xobject_resource_name(&content, &reserved), "XIns2");
    }

    #[test]
    fn a_font_name_claimed_by_a_queued_insertion_is_skipped() {
        let content = empty_page();
        let reserved = vec!["FIns1".to_string()];

        assert_eq!(unused_font_resource_name(&content, &reserved), "FIns2");
    }

    /// Both sources of "taken" have to compose: the page already paints
    /// `FIns1`, the log already claims `FIns2`, so the next free name is
    /// `FIns3`.
    #[test]
    fn page_content_and_queued_insertions_are_both_honoured() {
        let mut painted = run(1, 0.0, 0.0, 10.0, 10.0);
        painted.resource_font_name = "FIns1".to_string();
        let content = PageContent {
            text_runs: vec![painted],
            images: Vec::new(),
        };
        let reserved = vec!["FIns2".to_string()];

        assert_eq!(unused_font_resource_name(&content, &reserved), "FIns3");
    }

    #[test]
    fn queued_text_and_image_insertions_report_the_names_they_claim() {
        let mut inserted_run = run(1, 0.0, 0.0, 10.0, 10.0);
        inserted_run.resource_font_name = "FIns1".to_string();
        let mut inserted_image = image(1, 0.0, 0.0, 10.0, 10.0);
        inserted_image.resource_xobject_name = "XIns1".to_string();

        let mut document = pdf_document::Document::blank();
        let mut log = EditLog::new();
        log.apply(&mut document, Command::InsertTextRun(inserted_run));
        log.apply(
            &mut document,
            Command::InsertImage {
                item: inserted_image,
                source: None,
            },
        );

        assert_eq!(reserved_font_resource_names(&log), vec!["FIns1"]);
        assert_eq!(reserved_xobject_resource_names(&log), vec!["XIns1"]);
    }

    /// Removing an image leaves its XObject registered so undo can repaint
    /// it, so the name stays occupied even though nothing paints it any more
    /// — and `PageContent`, which only reports what is painted, cannot see
    /// that.
    #[test]
    fn a_queued_removal_still_reserves_its_xobject_name() {
        let mut removed = image(1, 0.0, 0.0, 10.0, 10.0);
        removed.resource_xobject_name = "XIns1".to_string();

        let mut document = pdf_document::Document::blank();
        let mut log = EditLog::new();
        log.apply(
            &mut document,
            Command::RemoveImage {
                item: removed,
                source: None,
            },
        );

        assert_eq!(reserved_xobject_resource_names(&log), vec!["XIns1"]);
        assert_eq!(
            unused_xobject_resource_name(&empty_page(), &reserved_xobject_resource_names(&log)),
            "XIns2"
        );
    }

    /// Annotation commands share the log with content commands, and claim no
    /// page resource at all — reserving a name for one would push every
    /// insertion onto a higher suffix for no reason.
    #[test]
    fn commands_that_claim_no_resource_reserve_nothing() {
        let mut document = pdf_document::Document::blank();
        let mut log = EditLog::new();
        log.apply(
            &mut document,
            Command::RotatePage {
                page: PageId(0),
                delta_degrees: 90,
            },
        );

        assert!(reserved_font_resource_names(&log).is_empty());
        assert!(reserved_xobject_resource_names(&log).is_empty());
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
