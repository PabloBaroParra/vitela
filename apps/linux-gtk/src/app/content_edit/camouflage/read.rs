//! What the core can say about a run's look — its style, and the family its
//! embedded font program registered as in Pango — for [`super::look_for`].

use std::cell::RefCell;
use std::sync::{Arc, Weak};

use gtk::pango;
use pdf_document::{PageId, TextRun};
use pdf_manip::LopdfDocument;

use super::fonts;
use crate::app::state::DocumentSession;

/// The font family a document's font resource resolved to, per
/// (document, page, resource), so opening the editor over the same font
/// again neither extracts its program nor registers it a second time. The
/// document is held weakly: one dropped takes its entries' meaning with it.
type FamilyCacheEntry = (Weak<LopdfDocument>, PageId, String, Option<String>);

thread_local! {
    static FAMILIES: RefCell<Vec<FamilyCacheEntry>> = const { RefCell::new(Vec::new()) };
}

/// What the core could say about a run's look.
pub(super) struct ReadStyle {
    pub(super) base_font: String,
    pub(super) family: Option<String>,
    pub(super) size_pt: Option<f64>,
    pub(super) colour: [f64; 3],
    pub(super) letter_spacing_pt: f64,
    /// The weight, slant and render mode the core read; none when it could
    /// not read the run, leaving the font's name to say.
    pub(super) hints: fonts::FaceHints,
}

pub(super) fn read_style(
    session: &DocumentSession,
    run: &TextRun,
    font_map: &pango::FontMap,
) -> ReadStyle {
    let mut read = ReadStyle {
        base_font: String::new(),
        family: None,
        size_pt: None,
        colour: [0.0; 3],
        letter_spacing_pt: 0.0,
        hints: fonts::FaceHints::default(),
    };
    let (Some(document), Some(backing)) = (
        session.document_model.as_ref(),
        session.save_backing.as_ref(),
    ) else {
        return read;
    };
    let sources: Vec<_> = session
        .imported_sources
        .iter()
        .map(|source| (source.id, &source.document))
        .collect();
    let sources = pdf_save::ImportedSources::new(&sources);

    match pdf_save::text_run_style_of(document, &backing.base, sources, run) {
        Ok(Some(style)) => {
            read.base_font = style.base_font;
            read.size_pt = Some(style.font_size);
            read.colour = style.fill_rgb;
            read.letter_spacing_pt = style.char_spacing;
            read.hints = fonts::FaceHints {
                bold: style.bold,
                italic: style.italic,
                render_mode: style.render_mode,
            };
        }
        _ => {
            read.base_font =
                pdf_save::page_font_families_of(document, run.page, &backing.base, sources)
                    .ok()
                    .and_then(|families| families.get(&run.resource_font_name).cloned())
                    .unwrap_or_default();
        }
    }

    read.family = cached_family(&backing.base, run).unwrap_or_else(|| {
        let family = pdf_save::text_run_font_program_of(document, &backing.base, sources, run)
            .ok()
            .flatten()
            .and_then(|program| fonts::register(font_map, &program));
        remember_family(&backing.base, run, family.clone());
        family
    });
    read
}

/// The family cached for `run`'s font in `base`: `Some(None)` when it is
/// known to have none, `None` when it has not been looked up.
fn cached_family(base: &Arc<LopdfDocument>, run: &TextRun) -> Option<Option<String>> {
    FAMILIES.with(|families| {
        let mut families = families.borrow_mut();
        families.retain(|(document, ..)| document.strong_count() > 0);
        families
            .iter()
            .find(|(document, page, resource, _)| {
                std::ptr::eq(document.as_ptr(), Arc::as_ptr(base))
                    && *page == run.page
                    && *resource == run.resource_font_name
            })
            .map(|(.., family)| family.clone())
    })
}

fn remember_family(base: &Arc<LopdfDocument>, run: &TextRun, family: Option<String>) {
    FAMILIES.with(|families| {
        families.borrow_mut().push((
            Arc::downgrade(base),
            run.page,
            run.resource_font_name.clone(),
            family,
        ));
    });
}
