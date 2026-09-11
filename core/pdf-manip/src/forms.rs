//! Rebuilding the destination's `/AcroForm` around an imported field
//! (checklist "Estructuras de documento", `docs/batch-pdf-assembly.md`
//! section 4).
//!
//! ## Why a form cannot simply be copied
//!
//! A form field is three objects in three places. The **widget** hangs off
//! the page's `/Annots` and is the only part a page copy brings along on its
//! own. The **field** behind it holds the name, the value and the default
//! appearance; in the common single-widget shape it *is* the widget, and
//! otherwise it sits above one or more widgets as their `/Parent`. The
//! **`/AcroForm`** lists every root field and holds the defaults the fields
//! inherit — and it lives in the catalog, which a graft must not copy (see
//! [`crate::graft`]).
//!
//! So copying the page gives a widget whose field nothing lists: a box that
//! looks like a field, is not one, and cannot be filled. That is why this
//! used to be refused outright. Accepting it means rebuilding the third part
//! in the destination, which is what this module does.
//!
//! ## The two names that collide
//!
//! Both of the things a form addresses **by name** can already be taken in
//! the destination, and in both cases the silent outcome is the wrong one: a
//! field name (`/T`) merges two fields into one, and a default resource name
//! (`/DR`) restyles a field without touching it. This module plans the merge
//! and sequences it; each collision and its reasoning live next to the code
//! that resolves it, in [`crate::form_fields`] and [`crate::form_resources`].
//!
//! ## What still cannot be accepted
//!
//! An **XFA** form ([`ManipError::SourceHasXfaForm`]) and a widget written
//! **inline** in `/Annots` ([`ManipError::SourceHasFormFields`]) — see
//! [`crate::report`], which owns both refusals.

use std::collections::HashSet;

use lopdf::{Dictionary, Document as LopdfRawDocument, Object, ObjectId};

use crate::form_fields::{inherit_form_defaults, list_fields, prune_kids};
use crate::form_resources::{merge_default_resources, rebind_default_appearance};
use crate::report::GraftWarning;

/// How far a `/Parent` or `/Kids` chain is followed before the field tree is
/// treated as malformed, matching [`crate::signatures`]'s own cap on the same
/// walk: real field trees nest a handful of levels, and the bound is what
/// keeps a cyclic one from spinning.
pub(crate) const MAX_FIELD_DEPTH: usize = 32;

/// What a renamed field's name gains. No `.`: a period is the separator of a
/// fully qualified field name (PDF 32000-1:2008 section 12.7.3.2), so one in
/// a `/T` would invent a hierarchy that is not there.
pub(crate) const IMPORTED_SUFFIX: &str = "-imported";

/// The widgets a page carries, split by whether they are objects of their
/// own. An **inline** one has no object id, so nothing can point at it and it
/// cannot be listed in a form — [`crate::report`] refuses those.
pub(crate) struct PageWidgets {
    pub(crate) indirect: Vec<ObjectId>,
    pub(crate) inline: usize,
}

impl PageWidgets {
    pub(crate) fn is_empty(&self) -> bool {
        self.indirect.is_empty() && self.inline == 0
    }
}

/// Every `/Subtype /Widget` annotation on `page`, resolving an `/Annots` that
/// is itself an indirect object — a shape a producer is free to write and a
/// reader that only accepts a direct array would silently see as "no fields".
pub(crate) fn widgets_on_page(doc: &LopdfRawDocument, page: &Dictionary) -> PageWidgets {
    let Some(annots) = page
        .get(b"Annots")
        .ok()
        .and_then(|value| dereference(doc, value))
        .and_then(|value| value.as_array().ok().cloned())
    else {
        return PageWidgets {
            indirect: Vec::new(),
            inline: 0,
        };
    };

    let mut widgets = PageWidgets {
        indirect: Vec::new(),
        inline: 0,
    };
    for annot in &annots {
        let resolved = dereference(doc, annot);
        let is_widget = resolved
            .as_ref()
            .and_then(|value| value.as_dict().ok())
            .and_then(|dict| dict.get(b"Subtype").and_then(|s| s.as_name()).ok())
            == Some(b"Widget".as_slice());
        if !is_widget {
            continue;
        }
        match annot.as_reference() {
            Ok(id) => widgets.indirect.push(id),
            Err(_) => widgets.inline += 1,
        }
    }
    widgets
}

/// True when the source's form is an XFA form, whose real definition is the
/// XML in the catalog and whose AcroForm fields are only its fallback shell.
pub(crate) fn form_is_xfa(doc: &LopdfRawDocument) -> bool {
    acroform(doc).is_some_and(|form| form.get(b"XFA").is_ok())
}

/// Everything the merge needs, worked out against the source before a single
/// object is copied — so the copy can be told what *not* to bring.
pub(crate) struct FormPlan {
    /// `(page, widget)` for every widget sitting on a selected page.
    widgets: Vec<(ObjectId, ObjectId)>,
    /// Root field per import, with the caller's 0-based source page index.
    /// A field shared by two selected pages appears once, under the first.
    roots: Vec<(usize, ObjectId)>,
    /// Every node of the imported field trees that must survive.
    keep: HashSet<ObjectId>,
    /// Kids of those fields that live on pages *not* being imported. Copying
    /// one would leave a widget with no page to draw on, silently sharing the
    /// field's value from nowhere, so the copy stops at them.
    pub(crate) foreign: HashSet<ObjectId>,
    /// Whether some imported widget arrives with no appearance stream.
    needs_appearances: bool,
}

impl FormPlan {
    pub(crate) fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }
}

pub(crate) fn plan(donor: &LopdfRawDocument, selected: &[ObjectId], pages: &[usize]) -> FormPlan {
    let mut widgets = Vec::new();
    let mut owners = Vec::new();
    for (position, &page_id) in selected.iter().enumerate() {
        let Ok(page) = donor.get_dictionary(page_id) else {
            continue;
        };
        for widget in widgets_on_page(donor, page).indirect {
            widgets.push((page_id, widget));
            owners.push((pages[position], widget));
        }
    }

    let mut keep: HashSet<ObjectId> = owners.iter().map(|&(_, widget)| widget).collect();
    let mut roots: Vec<(usize, ObjectId)> = Vec::new();
    let mut seen: HashSet<ObjectId> = HashSet::new();
    for (page, widget) in owners {
        let chain = field_ancestry(donor, widget);
        keep.extend(chain.iter().copied());
        let root = *chain.last().unwrap_or(&widget);
        if seen.insert(root) {
            roots.push((page, root));
        }
    }

    let mut foreign = HashSet::new();
    for &(_, root) in &roots {
        collect_foreign_kids(donor, root, &keep, &mut foreign, 0);
    }
    let needs_appearances = widgets.iter().any(|&(_, widget)| {
        donor
            .get_dictionary(widget)
            .map(|dict| dict.get(b"AP").is_err())
            .unwrap_or(false)
    });

    FormPlan {
        widgets,
        roots,
        keep,
        foreign,
        needs_appearances,
    }
}

/// The widget's field chain, nearest ancestor first, ending at the root
/// field. Empty when the widget *is* the field, which is the common shape.
fn field_ancestry(donor: &LopdfRawDocument, widget: ObjectId) -> Vec<ObjectId> {
    let mut chain = Vec::new();
    let mut current = widget;
    for _ in 0..MAX_FIELD_DEPTH {
        let Some(parent) = donor
            .get_dictionary(current)
            .ok()
            .and_then(|dict| dict.get(b"Parent").and_then(|p| p.as_reference()).ok())
        else {
            break;
        };
        // A `/Parent` that leads to the page tree is not a field parent; it
        // is the page the annotation sits on, written by a producer that
        // confused the two keys.
        let is_field = donor
            .get_object(parent)
            .map(|object| !matches!(object.type_name().unwrap_or_default(), b"Page" | b"Pages"))
            .unwrap_or(false);
        if !is_field || chain.contains(&parent) {
            break;
        }
        chain.push(parent);
        current = parent;
    }
    chain
}

fn collect_foreign_kids(
    donor: &LopdfRawDocument,
    node: ObjectId,
    keep: &HashSet<ObjectId>,
    foreign: &mut HashSet<ObjectId>,
    depth: usize,
) {
    if depth >= MAX_FIELD_DEPTH {
        return;
    }
    let Ok(kids) = donor
        .get_dictionary(node)
        .and_then(|dict| dict.get(b"Kids").and_then(|kids| kids.as_array()))
    else {
        return;
    };
    for kid in kids {
        let Ok(id) = kid.as_reference() else {
            continue;
        };
        if keep.contains(&id) {
            collect_foreign_kids(donor, id, keep, foreign, depth + 1);
        } else {
            foreign.insert(id);
        }
    }
}

/// Lists the imported fields in `doc`'s `/AcroForm`, creating that dictionary
/// when the destination had no form, and returns what had to be renamed.
///
/// Runs after the pages and their object graph are already in `doc`, because
/// every field node it edits is one of the objects that copy brought along.
pub(crate) fn apply(
    doc: &mut LopdfRawDocument,
    donor: &LopdfRawDocument,
    plan: &FormPlan,
) -> Vec<GraftWarning> {
    if plan.is_empty() {
        return Vec::new();
    }
    let donor_form = acroform(donor).unwrap_or_default();
    let (mut form, form_id) = destination_form(doc);

    for &(page_id, widget) in &plan.widgets {
        if let Ok(dict) = doc.get_dictionary_mut(widget) {
            dict.set("P", page_id);
        }
    }
    for &node in &plan.keep {
        prune_kids(doc, node, &plan.keep);
    }
    for &(_, root) in &plan.roots {
        inherit_form_defaults(doc, root, &donor_form);
    }

    let rebound = merge_default_resources(doc, donor, &mut form, &donor_form);
    if !rebound.is_empty() {
        let mut nodes: Vec<ObjectId> = plan.keep.iter().copied().collect();
        nodes.sort();
        for node in nodes {
            rebind_default_appearance(doc, node, &rebound);
        }
    }

    let warnings = list_fields(doc, &mut form, &plan.roots);
    if plan.needs_appearances {
        form.set("NeedAppearances", true);
    }
    store_form(doc, form, form_id);
    warnings
}

fn acroform(doc: &LopdfRawDocument) -> Option<Dictionary> {
    dereferenced_dict(doc, doc.catalog().ok()?.get(b"AcroForm").ok()?)
}

/// The destination's form dictionary and, when it is an indirect object, the
/// id to write it back to.
fn destination_form(doc: &LopdfRawDocument) -> (Dictionary, Option<ObjectId>) {
    let Ok(catalog) = doc.catalog() else {
        return (Dictionary::new(), None);
    };
    let Ok(form) = catalog.get(b"AcroForm") else {
        return (Dictionary::new(), None);
    };
    match form.as_reference() {
        Ok(id) => (
            doc.get_dictionary(id).cloned().unwrap_or_default(),
            Some(id),
        ),
        Err(_) => (form.as_dict().cloned().unwrap_or_default(), None),
    }
}

fn store_form(doc: &mut LopdfRawDocument, form: Dictionary, form_id: Option<ObjectId>) {
    match form_id {
        Some(id) => {
            doc.objects.insert(id, Object::Dictionary(form));
        }
        None => {
            let Ok(catalog_id) = doc
                .trailer
                .get(b"Root")
                .and_then(|root| root.as_reference())
            else {
                return;
            };
            if let Ok(catalog) = doc.get_dictionary_mut(catalog_id) {
                catalog.set("AcroForm", form);
            }
        }
    }
}

pub(crate) fn dereference(doc: &LopdfRawDocument, value: &Object) -> Option<Object> {
    match value {
        Object::Reference(id) => doc.get_object(*id).ok().cloned(),
        other => Some(other.clone()),
    }
}

pub(crate) fn dereferenced_dict(doc: &LopdfRawDocument, value: &Object) -> Option<Dictionary> {
    dereference(doc, value)?.as_dict().ok().cloned()
}
