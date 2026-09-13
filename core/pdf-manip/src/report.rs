//! What an import costs, decided before it happens (checklist "Estructuras de
//! documento", `docs/batch-pdf-assembly.md` section 4).
//!
//! ## The rule this module exists to keep
//!
//! *No import that loses information may look like an import that did not.*
//!
//! A page carries more than its content stream. It sits inside a document
//! that owns an `/AcroForm`, a name tree of destinations, an outline, an
//! optional-content configuration and a tagged-structure tree — all of them
//! in the **catalog**, and the catalog is exactly what a graft must not copy
//! (see [`crate::graft`]). Every one of those is therefore a thing that can
//! be quietly left behind.
//!
//! Each is sorted into one of two answers, and there is no third:
//!
//! **Refused** ([`ManipError`]) when the imported page would come out
//! *wrong*, not merely poorer:
//!
//! - a **signature** widget — the one form field that is refused rather than
//!   merged: the appearance travels, the signature dictionary and the byte
//!   range it covers do not, so the imported page would show a signature
//!   block attesting to a file that is not there. See [`crate::signatures`].
//! - an **XFA** form, whose fields are only the fallback shell of a form
//!   defined by XML in the catalog — merge the shell and they look right and
//!   behave differently; and a field written **inline** in `/Annots`, which
//!   has no object id to list in a form. Every other field is merged instead,
//!   by [`crate::forms`].
//! - optional content (`/OCG`, `/OCMD`). Its visibility configuration lives
//!   in the catalog's `/OCProperties`. Import the marked content without it
//!   and a viewer has no configuration saying the layer was off — content the
//!   author hid can come back visible. That is not lost data, it is wrong
//!   output, and it is worse.
//!
//! **Reported** ([`GraftWarning`]) when the page's own content is intact and
//! something *about* it did not come along. Refusing these would refuse most
//! real documents — nearly every office PDF is tagged — so they are named
//! instead, and the caller decides.
//!
//! A caller that wants the answer before committing calls
//! [`graft_report`]; [`crate::graft_pages`] runs the very same inspection
//! before it copies anything, so the two can never disagree about what a
//! given import costs. Everything decided here is a fact about the *source*
//! alone — which is what lets the question be asked with no destination in
//! hand — bar one that cannot be: whether a field name is already taken on
//! the other side. [`crate::graft_pages`] appends that one itself.

use std::collections::{BTreeSet, HashSet};
use std::fmt;

use lopdf::{Dictionary, Document as LopdfRawDocument, ObjectId};

use crate::destinations::{resolve_destination, DestinationTarget};
use crate::document::LopdfDocument;
use crate::error::ManipError;
use crate::forms;
use crate::graft::selected_pages;
use crate::links;
use crate::page_graph::{collect_reachable, flattened_page};
use crate::signatures;

/// Something an import carries out that the imported pages will not have.
///
/// Every `page` is the 0-based index into the *source* document, exactly as
/// the caller passed it to [`graft_report`] or [`crate::graft_pages`] — the
/// same convention [`ManipError::SourceHasFormFields`] uses.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GraftWarning {
    /// A link on the imported page points at a source page that is not in the
    /// selection. Importing that page too would drag most of the source in
    /// behind one link, so the reference is left pointing at an absent
    /// object — a reference to null (PDF 32000-1:2008 section 7.3.10), which
    /// makes the link inert rather than the file corrupt.
    LinkTargetNotImported { page: usize },
    /// A named destination on the imported page could not be rewritten into
    /// an explicit one, so the link no longer resolves: either the source
    /// never defined the name, or it sits on an annotation written inline in
    /// `/Annots` and so has no object of its own to rewrite.
    NamedDestinationDropped { page: usize, name: String },
    /// The source's outline (bookmarks) has entries pointing at pages in this
    /// selection. The outline tree belongs to the source catalog and is not
    /// imported.
    OutlinesNotImported { entries: usize },
    /// The imported page carries tagged-structure marks whose structure tree
    /// stays in the source. The page's content is intact; the reading order
    /// and semantics assistive technology reads are not.
    TaggedStructureNotImported { page: usize },
    /// An imported form field had to be renamed: two top-level fields sharing
    /// one `/T` are one field with one value between them (PDF 32000-1:2008
    /// section 12.7.3.2), so keeping the name would have merged the import
    /// into the destination's field instead of adding it. See
    /// [`crate::form_fields`].
    FormFieldRenamed {
        page: usize,
        from: String,
        to: String,
    },
    /// The source document is signed, and none of that signing travels with
    /// its pages (checklist "Seguridad y firmas",
    /// `docs/batch-pdf-assembly.md` section 5).
    ///
    /// Not narrowed to particular pages, unlike every warning above it: a
    /// signature's `/ByteRange` covers the whole file, so it is a fact about
    /// the source and equally true of any page taken out of it. The source
    /// file itself is untouched and still verifies — what the user needs to
    /// know is that the copy they are assembling does not inherit that.
    SourceSignaturesNotImported,
}

impl fmt::Display for GraftWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GraftWarning::LinkTargetNotImported { page } => write!(
                f,
                "page {page} links to a page that is not part of this import; the link will not work"
            ),
            GraftWarning::NamedDestinationDropped { page, name } => write!(
                f,
                "page {page} has a link to the named destination \"{name}\", which cannot be carried over; the link will not work"
            ),
            GraftWarning::OutlinesNotImported { entries: 1 } => write!(
                f,
                "1 bookmark points at the imported pages and stays in the source document"
            ),
            GraftWarning::OutlinesNotImported { entries } => write!(
                f,
                "{entries} bookmarks point at the imported pages and stay in the source document"
            ),
            GraftWarning::TaggedStructureNotImported { page } => write!(
                f,
                "page {page} loses its tagged structure; its content is imported in full, its accessibility structure is not"
            ),
            GraftWarning::FormFieldRenamed { page, from, to } => write!(
                f,
                "the form field \"{from}\" on page {page} was imported as \"{to}\": the document already has a field called \"{from}\", and two fields of one name would share a single value"
            ),
            GraftWarning::SourceSignaturesNotImported => write!(
                f,
                "the source document is signed; the imported pages carry their content but none of its signature"
            ),
        }
    }
}

/// Everything an import will leave behind, or empty when it leaves nothing.
///
/// An empty report is the *only* way to know an import is lossless: an import
/// that would lose something either refuses ([`ManipError`]) or says so here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraftReport {
    warnings: Vec<GraftWarning>,
}

impl GraftReport {
    pub fn warnings(&self) -> &[GraftWarning] {
        &self.warnings
    }

    /// True when the import carries everything across.
    pub fn is_lossless(&self) -> bool {
        self.warnings.is_empty()
    }

    /// Adds a warning the inspection could not have known about.
    ///
    /// Everything [`inspect`] reports is a fact about the *source* alone, so
    /// that [`graft_report`] can answer without a destination in hand. A
    /// field rename is the one consequence that needs both documents — it
    /// only exists because a name is taken on the other side — so the graft
    /// itself appends it once it has seen both.
    pub(crate) fn push(&mut self, warning: GraftWarning) {
        self.warnings.push(warning);
    }
}

/// A completed import: the resulting document, and what it left behind.
///
/// The report travels *with* the document rather than beside it so a caller
/// cannot take one without seeing the other.
#[derive(Debug, Clone)]
pub struct GraftOutcome {
    pub document: LopdfDocument,
    pub report: GraftReport,
}

/// What importing `pages` out of `source` would leave behind — without
/// importing anything.
///
/// This is the gate an import flow calls at selection time, so a user learns
/// the cost while they can still change their mind. It refuses exactly what
/// [`crate::graft_pages`] refuses, because that function calls straight into
/// this one, and warns everything it warns bar the field renames only the
/// destination can reveal.
pub fn graft_report(source: &LopdfDocument, pages: &[usize]) -> Result<GraftReport, ManipError> {
    let selected = selected_pages(&source.0, pages)?;
    inspect(&source.0, &selected, pages)
}

/// The shared inspection. `selected` is `pages` already resolved to object
/// ids in `donor`, so this works equally on the source document and on the
/// renumbered clone `graft_pages` copies out of.
pub(crate) fn inspect(
    donor: &LopdfRawDocument,
    selected: &[ObjectId],
    pages: &[usize],
) -> Result<GraftReport, ManipError> {
    let selected_set: HashSet<ObjectId> = selected.iter().copied().collect();
    let mut warnings = Vec::new();

    for (i, &page_id) in selected.iter().enumerate() {
        let page = pages[i];
        let dict = flattened_page(donor, page_id)?;
        // Asked before the general widget check, which would also match: a
        // signature is a form field, and answering "this page has form
        // fields" would hide the only part that mattered.
        if signatures::page_has_signature_widget(donor, &dict) {
            return Err(ManipError::SourceHasSignature(page));
        }
        let widgets = forms::widgets_on_page(donor, &dict);
        if !widgets.is_empty() && forms::form_is_xfa(donor) {
            return Err(ManipError::SourceHasXfaForm(page));
        }
        if widgets.inline > 0 {
            return Err(ManipError::SourceHasFormFields(page));
        }
        if page_uses_optional_content(donor, &dict) {
            return Err(ManipError::SourceHasOptionalContent(page));
        }
        if page_is_tagged(donor, &dict) {
            warnings.push(GraftWarning::TaggedStructureNotImported { page });
        }
        collect_destination_warnings(donor, &dict, &selected_set, page, &mut warnings);
    }

    let entries = links::outline_entries_into(donor, &selected_set);
    if entries > 0 {
        warnings.push(GraftWarning::OutlinesNotImported { entries });
    }
    // Document-level, so it is asked once rather than per page — and only
    // once no page has been refused: a selection that includes the signed
    // page never reaches here.
    if signatures::raw_document_has_signatures(donor) {
        warnings.push(GraftWarning::SourceSignaturesNotImported);
    }
    Ok(GraftReport { warnings })
}

/// Classifies every destination the page carries: a page in the selection is
/// fine, anything else is named.
fn collect_destination_warnings(
    donor: &LopdfRawDocument,
    dict: &Dictionary,
    selected: &HashSet<ObjectId>,
    page: usize,
    warnings: &mut Vec<GraftWarning>,
) {
    for slot in links::destination_slots(donor, dict) {
        let Some(value) = links::destination_value(donor, slot) else {
            continue;
        };
        match resolve_destination(donor, &value) {
            DestinationTarget::Page { page: target, .. } if selected.contains(&target) => {}
            DestinationTarget::Page { .. } => {
                warnings.push(GraftWarning::LinkTargetNotImported { page })
            }
            DestinationTarget::UnresolvedName(name) => {
                warnings.push(GraftWarning::NamedDestinationDropped { page, name })
            }
            DestinationTarget::Other => {}
        }
    }
    for name in links::inline_named_destinations(donor, dict) {
        warnings.push(GraftWarning::NamedDestinationDropped { page, name });
    }
}

/// True when anything the page reaches is an optional-content group or
/// membership dictionary (PDF 32000-1:2008 section 8.11).
///
/// Detected by `/Type` on the target rather than by the `/OC` key on the
/// referring XObject or annotation, because every `/OC` leads to one of these
/// two and one predicate cannot miss a producer that puts `/OC` somewhere
/// this crate did not think to look.
fn page_uses_optional_content(donor: &LopdfRawDocument, dict: &Dictionary) -> bool {
    let mut reachable = BTreeSet::new();
    collect_reachable(donor, dict, &HashSet::new(), &mut reachable);
    reachable.iter().any(|&id| {
        donor
            .get_object(id)
            .map(|object| matches!(object.type_name().unwrap_or_default(), b"OCG" | b"OCMD"))
            .unwrap_or(false)
    })
}

/// True when the page carries marked content keyed into a structure tree the
/// source owns. Both halves are required: `/StructParents` without a
/// `/StructTreeRoot` keys into nothing, and there is nothing to lose.
fn page_is_tagged(donor: &LopdfRawDocument, dict: &Dictionary) -> bool {
    dict.get(b"StructParents").is_ok()
        && donor
            .catalog()
            .map(|catalog| catalog.get(b"StructTreeRoot").is_ok())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_report_is_lossless() {
        let report = GraftReport::default();
        assert!(report.is_lossless());
        assert!(report.warnings().is_empty());
    }

    #[test]
    fn a_warning_names_the_page_it_is_about() {
        assert!(GraftWarning::LinkTargetNotImported { page: 2 }
            .to_string()
            .contains("page 2"));
        assert!(GraftWarning::TaggedStructureNotImported { page: 1 }
            .to_string()
            .contains("page 1"));
    }

    #[test]
    fn a_dropped_named_destination_names_the_destination() {
        let message = GraftWarning::NamedDestinationDropped {
            page: 0,
            name: "chapter2".into(),
        }
        .to_string();

        assert!(message.contains("chapter2"), "{message}");
        assert!(message.contains("page 0"), "{message}");
    }

    #[test]
    fn an_outline_warning_counts_the_entries_and_agrees_with_itself() {
        assert_eq!(
            GraftWarning::OutlinesNotImported { entries: 1 }.to_string(),
            "1 bookmark points at the imported pages and stays in the source document"
        );
        assert!(GraftWarning::OutlinesNotImported { entries: 3 }
            .to_string()
            .starts_with("3 bookmarks"));
    }
}
