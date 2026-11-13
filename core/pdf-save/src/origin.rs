//! Which document, and which object inside it, hold a model page's bytes.
//!
//! Everything that reads or probes a page's *content* used to answer this
//! question the same way: walk the base document's pages and take the
//! `PageId`-th one. That works for exactly as long as a `PageId` is a
//! position in the base document — which stops being true the moment a page
//! is imported. An imported page's bytes are in the PDF it came from, and its
//! id was allocated past every base page's; a positional walk either fails or,
//! worse, finds an unrelated base page.
//!
//! So the resolution moves here, where the model's own `PageOrigin` answers
//! it, and the parsing crates (`pdf-edit`) take an object id from a caller
//! that knows rather than deriving one from a position they cannot check.
//!
//! Batch PDF assembly §6.

use lopdf::ObjectId;
use pdf_document::{Document, PageContent, PageId, PageOrigin};
use pdf_manip::LopdfDocument;

use crate::bridge::ImportedSources;
use crate::error::SaveError;

/// Where a model page's content lives.
#[derive(Debug, Clone, Copy)]
pub enum PageBacking<'a> {
    /// The page's bytes are `object` in `document` — the base document for a
    /// page that came from the opened file, the source PDF for an imported
    /// one.
    Object {
        document: &'a LopdfDocument,
        object: ObjectId,
    },
    /// A blank page the model inserted. It has no backing bytes at all: it
    /// only becomes an object when a save materializes it.
    Empty,
}

/// Resolves `page` to the document and object that hold its content.
///
/// Fails rather than guesses in every case where the answer is unknown: a
/// `PageId` the model does not carry, an imported page whose source the
/// caller did not supply, or a page index past the end of the document that
/// was supposed to contain it. A content read that silently landed on the
/// wrong page would record an edit against bytes the user never saw.
pub fn page_backing<'a>(
    document: &Document,
    page: PageId,
    base: &'a LopdfDocument,
    sources: ImportedSources<'_, 'a>,
) -> Result<PageBacking<'a>, SaveError> {
    let origin = document
        .pages
        .iter()
        .find(|candidate| candidate.id == page)
        .map(|candidate| candidate.origin)
        .ok_or(SaveError::InvalidSaveRequest(
            "page id is not present in the document",
        ))?;

    match origin {
        PageOrigin::Blank => Ok(PageBacking::Empty),
        PageOrigin::Base { page_index } => Ok(PageBacking::Object {
            document: base,
            object: object_at(base, page_index).ok_or(SaveError::InvalidSaveRequest(
                "base page index is past the end of the opened document",
            ))?,
        }),
        PageOrigin::Imported { source, page_index } => {
            let imported = sources.get(source).ok_or(SaveError::InvalidSaveRequest(
                "imported page names a source this save was not given",
            ))?;
            Ok(PageBacking::Object {
                document: imported,
                object: object_at(imported, page_index).ok_or(SaveError::InvalidSaveRequest(
                    "imported page index is past the end of its source document",
                ))?,
            })
        }
    }
}

/// The page object at a zero-based index, or `None` past the end. lopdf keys
/// its page map by 1-based page *number*, which is the only place that
/// off-by-one belongs.
fn object_at(document: &LopdfDocument, page_index: u32) -> Option<ObjectId> {
    document
        .as_lopdf()
        .get_pages()
        .get(&(page_index + 1))
        .copied()
}

/// Reads a page's text runs and images through its origin — the read every
/// shell should be making, and the one that reaches an imported page.
///
/// The returned items are stamped with `page`, the id the *model* uses, not
/// with the position the bytes occupy in whichever document supplied them.
/// That is what lets an edit recorded here be replayed at save time against
/// the page the graft produces (`content::replay_content_edits`).
///
/// A blank page reads as empty rather than as an error: it genuinely paints
/// nothing, and a shell asking what is on it deserves that answer instead of
/// a failure it has to special-case.
///
/// Like [`crate::read_page_content`], the ids are positions in a parse of the
/// bytes as they stand now. Re-read after a save before acting on them again.
pub fn read_page_content_of(
    document: &Document,
    page: PageId,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
) -> Result<PageContent, SaveError> {
    match page_backing(document, page, base, sources)? {
        PageBacking::Empty => Ok(PageContent::default()),
        PageBacking::Object {
            document: backing,
            object,
        } => {
            pdf_edit::read_page_object_content(backing.as_lopdf(), object, page).map_err(Into::into)
        }
    }
}

/// [`read_page_content_of`] as the reader sees the page: the parse, with every
/// page-content edit still pending in `document`'s `EditLog` layered on top
/// (`pdf_edit::overlay_pending_content`).
///
/// This is the read to hit-test with. The preview a shell draws already
/// paints pending edits, so the parse alone describes a page nobody can see
/// any more: a retyped run with its old text and its old box. Items that
/// exist only because of a pending insertion carry a synthetic id
/// (`pdf_edit::PENDING_ITEM_ID_BASE`); a retyped run keeps its real one.
pub fn read_pending_page_content_of(
    document: &Document,
    page: PageId,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
) -> Result<PageContent, SaveError> {
    match page_backing(document, page, base, sources)? {
        PageBacking::Empty => Ok(PageContent::default()),
        PageBacking::Object {
            document: backing,
            object,
        } => {
            let mut content = pdf_edit::read_page_object_content(backing.as_lopdf(), object, page)?;
            pdf_edit::overlay_pending_content(
                &mut content,
                &document.pending_edits,
                page,
                backing.as_lopdf(),
                object,
            );
            Ok(content)
        }
    }
}

/// The batch that retypes the text block `runs` make up to `text`: a one-line
/// block as [`pdf_edit::retype_block`] builds it, a paragraph wrapped to its
/// width by [`pdf_edit::retype_paragraph`], measured in the fonts of the
/// page's origin.
///
/// `runs` are the block's runs as the reader sees them
/// ([`read_pending_page_content_of`]), all on one page; `None` when there are
/// none.
pub fn retype_text_block_of(
    document: &Document,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
    runs: &[pdf_document::TextRun],
    text: &str,
    with_inserted_font: bool,
) -> Result<Option<pdf_document::Command>, SaveError> {
    let Some(first) = runs.first() else {
        return Ok(None);
    };
    match page_backing(document, first.page, base, sources)? {
        PageBacking::Empty => Ok(pdf_edit::retype_block(runs, text, with_inserted_font)),
        PageBacking::Object {
            document: backing,
            object,
        } => pdf_edit::retype_paragraph(backing.as_lopdf(), object, runs, text, with_inserted_font)
            .map_err(Into::into),
    }
}

/// The `/BaseFont` name of every font a page's resources declare, resolved
/// through the page's origin — the [`crate::page_font_families`] an overlay
/// should use once a session can hold imported pages.
pub fn page_font_families_of(
    document: &Document,
    page: PageId,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
) -> Result<std::collections::BTreeMap<String, String>, SaveError> {
    match page_backing(document, page, base, sources)? {
        PageBacking::Empty => Ok(std::collections::BTreeMap::new()),
        PageBacking::Object {
            document: backing,
            object,
        } => pdf_edit::page_object_font_families(backing.as_lopdf(), object).map_err(Into::into),
    }
}

/// How `run` looks on its page, read through the page's origin — what a
/// shell's editor needs to draw typed text in the run's own font, size and
/// colour ([`pdf_edit::text_run_style`]). `None` for a page with no content.
pub fn text_run_style_of(
    document: &Document,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
    run: &pdf_document::TextRun,
) -> Result<Option<pdf_edit::TextRunStyle>, SaveError> {
    match page_backing(document, run.page, base, sources)? {
        PageBacking::Empty => Ok(None),
        PageBacking::Object {
            document: backing,
            object,
        } => pdf_edit::text_run_style(backing.as_lopdf(), object, run)
            .map(Some)
            .map_err(Into::into),
    }
}

/// How `run` looks — bold, italic — and which of those can be taken off it,
/// read through the page's origin ([`pdf_edit::text_run_look`]). What a
/// shell's bold / italic toggles show before the reader presses one. `None`
/// for a page with no content.
pub fn text_run_look_of(
    document: &Document,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
    run: &pdf_document::TextRun,
) -> Result<Option<pdf_edit::RunLook>, SaveError> {
    match page_backing(document, run.page, base, sources)? {
        PageBacking::Empty => Ok(None),
        PageBacking::Object {
            document: backing,
            object,
        } => pdf_edit::text_run_look(backing.as_lopdf(), object, run)
            .map(Some)
            .map_err(Into::into),
    }
}

/// The font program embedded for `run`'s font, read through the page's
/// origin ([`pdf_edit::text_run_font_program`]). `None` for a font that
/// embeds none, and for a page with no content.
pub fn text_run_font_program_of(
    document: &Document,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
    run: &pdf_document::TextRun,
) -> Result<Option<pdf_edit::FontProgram>, SaveError> {
    match page_backing(document, run.page, base, sources)? {
        PageBacking::Empty => Ok(None),
        PageBacking::Object {
            document: backing,
            object,
        } => pdf_edit::text_run_font_program(backing.as_lopdf(), object, run).map_err(Into::into),
    }
}
