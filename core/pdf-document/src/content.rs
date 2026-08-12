//! Page-content data model (T-148) — the text runs and images that live
//! *inside* a page's content stream, as opposed to annotations and form
//! fields, which are addressable objects hanging off `/Annots`/`/AcroForm`.
//!
//! Parsing a content stream to produce these values is `pdf-edit`'s job
//! (Batch 21, T-152) — this crate only models the data.
//!
//! **This model is deliberately NOT owned by `Document`/`Page`** (batch
//! decision 2): unlike `AnnotationSet`, page content is loaded lazily, the
//! first time a shell enters content-edit mode for a given page, because
//! interpreting every page's content stream at open time is wasted work for
//! the majority of sessions that never edit page content. `PageContent` is
//! therefore a *snapshot* returned by the parser, not document state.
//!
//! `Rect` is reused from [`crate::annotation`] rather than redefined: a
//! bounding box in page space means the same thing here as it does there.

use crate::annotation::Rect;
use crate::document::PageId;

/// Identifies a text run or an image **within one page's parsed content**.
///
/// Unlike [`crate::AnnotationId`], this is not a document-wide identity: page
/// content is not addressable in the PDF file (it is a byte range inside a
/// content stream), so the id is assigned by the parser and is only
/// meaningful for the `PageContent` snapshot it came from. Text runs and
/// images are numbered independently — the pair (kind, id) is what
/// disambiguates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentItemId(pub u64);

/// Which font machinery the run's active font uses, which is what decides
/// whether replacement text can be encoded at all.
///
/// This is metadata, not permission: per batch decision 3 a run is not
/// statically "editable" or not — a simple font may accept `café` and reject
/// `日本語`. `pdf-edit`'s encoder answers that per attempt. What this enum
/// carries is the one case that is rejected outright in v1
/// (`EmbeddedComposite`, i.e. Type0/CID), because extending a subsetted CID
/// font's glyph coverage means re-subsetting it.
///
/// `#[non_exhaustive]`: Type0/CID editing is explicitly a post-v1 candidate,
/// and lifting it will likely need to distinguish CID font flavours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FontKind {
    /// One of the 14 standard fonts — never embedded, always encodable
    /// through a known simple encoding.
    Standard14,
    /// An embedded font with a simple (single-byte) `/Encoding`: WinAnsi,
    /// MacRoman, or a resolvable `/Differences` array.
    EmbeddedSimple,
    /// A composite Type0/CID font. Read-only for text editing in v1.
    EmbeddedComposite,
}

/// A contiguous run of text painted by one show-text operator (`Tj`/`TJ`/
/// `'`/`"`) under a single active font.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub id: ContentItemId,
    pub page: PageId,
    /// Bounding box in page space (points, origin bottom-left) — the result
    /// of applying the text matrix and the current transformation matrix, so
    /// it is directly comparable with an annotation `Rect` and correct under
    /// page rotation.
    pub bbox: Rect,
    /// The font's key in the page's `/Resources /Font` dictionary (e.g.
    /// `F1`), which is how `pdf-edit` reaches the font to encode against.
    pub resource_font_name: String,
    pub font_kind: FontKind,
    /// The decoded text as shown on the page.
    pub text: String,
}

/// An image painted by a `Do` operator against an XObject in the page's
/// resources.
///
/// The image *bytes* are deliberately absent: they live in the file and can
/// be large, and the only operation that needs them is
/// `Command::ReplaceImageSource`, which carries them explicitly.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageItem {
    pub id: ContentItemId,
    pub page: PageId,
    /// Bounding box in page space — the unit square mapped through the `cm`
    /// matrix in effect at the `Do`.
    pub bbox: Rect,
    /// The XObject's key in the page's `/Resources /XObject` dictionary.
    pub resource_xobject_name: String,
}

/// The parsed content of a single page: what a shell needs to hit-test and
/// edit, and nothing more.
///
/// A read-only snapshot with public fields, unlike [`crate::AnnotationSet`]:
/// nothing mutates a `PageContent` in place. Edits are recorded as
/// [`crate::Command`] values and applied to the file at save time by
/// `pdf-edit`; the shell obtains the new state by re-reading the page after
/// the save (batch decision 6).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageContent {
    pub text_runs: Vec<TextRun>,
    pub images: Vec<ImageItem>,
}

impl PageContent {
    /// The text run carrying `id`, if this page has one. Backed by a linear
    /// scan, matching `AnnotationSet`: per-page item counts are small and
    /// parse order must be preserved, since it is paint order.
    pub fn text_run(&self, id: ContentItemId) -> Option<&TextRun> {
        self.text_runs.iter().find(|run| run.id == id)
    }

    /// The image carrying `id`, if this page has one.
    pub fn image(&self, id: ContentItemId) -> Option<&ImageItem> {
        self.images.iter().find(|image| image.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 100.0,
            height: 12.0,
        }
    }

    fn sample_text_run(id: u64) -> TextRun {
        TextRun {
            id: ContentItemId(id),
            page: PageId(0),
            bbox: rect(72.0, 700.0),
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::Standard14,
            text: "Hello".to_string(),
        }
    }

    fn sample_image(id: u64) -> ImageItem {
        ImageItem {
            id: ContentItemId(id),
            page: PageId(0),
            bbox: rect(72.0, 400.0),
            resource_xobject_name: "Im1".to_string(),
        }
    }

    #[test]
    fn a_freshly_parsed_page_with_no_content_is_empty() {
        let content = PageContent::default();

        assert!(content.text_runs.is_empty());
        assert!(content.images.is_empty());
    }

    #[test]
    fn text_run_lookup_by_id_finds_the_run() {
        let content = PageContent {
            text_runs: vec![sample_text_run(1), sample_text_run(2)],
            images: Vec::new(),
        };

        let found = content
            .text_run(ContentItemId(2))
            .expect("id 2 was parsed on this page");
        assert_eq!(found.id, ContentItemId(2));
        assert!(content.text_run(ContentItemId(99)).is_none());
    }

    #[test]
    fn image_lookup_by_id_finds_the_image() {
        let content = PageContent {
            text_runs: Vec::new(),
            images: vec![sample_image(5)],
        };

        let found = content
            .image(ContentItemId(5))
            .expect("id 5 was parsed on this page");
        assert_eq!(found.resource_xobject_name, "Im1");
        assert!(content.image(ContentItemId(6)).is_none());
    }

    /// Ids are only unique within the page they were parsed from, so a text
    /// run and an image may legitimately share one. The two lookups must not
    /// be able to return each other's item.
    #[test]
    fn a_text_run_and_an_image_may_share_an_id_without_colliding() {
        let content = PageContent {
            text_runs: vec![sample_text_run(1)],
            images: vec![sample_image(1)],
        };

        assert_eq!(
            content.text_run(ContentItemId(1)).map(|run| &run.text),
            Some(&"Hello".to_string())
        );
        assert_eq!(
            content
                .image(ContentItemId(1))
                .map(|image| &image.resource_xobject_name),
            Some(&"Im1".to_string())
        );
    }

    /// Decision 3: whether a run can be edited depends on the replacement
    /// text, so `FontKind` is descriptive metadata, not an `editable` flag —
    /// the check lives in `pdf-edit`'s encoder. What the model must carry is
    /// enough information for that encoder to reject composite fonts.
    #[test]
    fn font_kind_distinguishes_the_composite_case_editing_rejects_in_v1() {
        let mut run = sample_text_run(1);
        run.font_kind = FontKind::EmbeddedComposite;

        assert_ne!(run.font_kind, FontKind::EmbeddedSimple);
        assert_ne!(run.font_kind, FontKind::Standard14);
    }
}
