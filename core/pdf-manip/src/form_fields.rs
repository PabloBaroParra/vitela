//! Listing an imported field in the destination's `/AcroForm`, and the name
//! collision that makes possible (checklist "Estructuras de documento",
//! `docs/batch-pdf-assembly.md` section 4).
//!
//! Two top-level fields sharing one `/T` are not two fields in
//! PDF 32000-1:2008 section 12.7.3.2 — they are one field with two widgets,
//! sharing a single value. So appending a colliding import to `/Fields` would
//! not add a field, it would merge the import into the destination's and let
//! one overwrite the other's value. The import is renamed instead, and said
//! out loud ([`GraftWarning::FormFieldRenamed`]): the name is what whoever
//! fills the form will see, so it is not a change to make in silence.
//!
//! The rest of this module is the bookkeeping that makes an imported field
//! stand on its own once the `/AcroForm` it grew up in is gone: the kids
//! whose pages were left behind are dropped, and the defaults it used to read
//! off that form are written onto it.

use std::collections::HashSet;

use lopdf::{Dictionary, Document as LopdfRawDocument, Object, ObjectId};

use crate::forms::{dereference, IMPORTED_SUFFIX, MAX_FIELD_DEPTH};
use crate::report::GraftWarning;

/// Appends every imported root to `/Fields`, renaming the ones whose name is
/// already taken.
pub(crate) fn list_fields(
    doc: &mut LopdfRawDocument,
    form: &mut Dictionary,
    roots: &[(usize, ObjectId)],
) -> Vec<GraftWarning> {
    let mut fields: Vec<Object> = form
        .get(b"Fields")
        .ok()
        .and_then(|value| dereference(doc, value))
        .and_then(|value| value.as_array().ok().cloned())
        .unwrap_or_default();
    let mut taken: HashSet<String> = fields
        .iter()
        .filter_map(|field| field_name(doc, field.as_reference().ok()?))
        .collect();

    let mut warnings = Vec::new();
    for &(page, root) in roots {
        fields.push(Object::Reference(root));
        let Some(name) = field_name(doc, root) else {
            continue;
        };
        let chosen = if taken.contains(&name) {
            free_name(&name, &taken)
        } else {
            name.clone()
        };
        if chosen != name {
            if let Ok(dict) = doc.get_dictionary_mut(root) {
                dict.set("T", Object::string_literal(chosen.as_str()));
            }
            warnings.push(GraftWarning::FormFieldRenamed {
                page,
                from: name,
                to: chosen.clone(),
            });
        }
        taken.insert(chosen);
    }
    form.set("Fields", fields);
    warnings
}

fn free_name(base: &str, taken: &HashSet<String>) -> String {
    let first = format!("{base}{IMPORTED_SUFFIX}");
    if !taken.contains(&first) {
        return first;
    }
    (2u32..)
        .take(MAX_FIELD_DEPTH.pow(2))
        .map(|n| format!("{base}{IMPORTED_SUFFIX}-{n}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or(first)
}

fn field_name(doc: &LopdfRawDocument, field: ObjectId) -> Option<String> {
    let name = doc
        .get_dictionary(field)
        .ok()?
        .get(b"T")
        .ok()?
        .as_str()
        .ok()?;
    Some(String::from_utf8_lossy(name).to_string())
}

/// Drops the `/Kids` whose pages were left behind, so a field arrives owning
/// exactly the widgets that came with it.
pub(crate) fn prune_kids(doc: &mut LopdfRawDocument, node: ObjectId, keep: &HashSet<ObjectId>) {
    let Ok(dict) = doc.get_dictionary_mut(node) else {
        return;
    };
    let Ok(kids) = dict.get(b"Kids").and_then(|kids| kids.as_array()) else {
        return;
    };
    let kept: Vec<Object> = kids
        .iter()
        .filter(|kid| {
            kid.as_reference()
                .map(|id| keep.contains(&id))
                .unwrap_or(true)
        })
        .cloned()
        .collect();
    dict.set("Kids", kept);
}

/// Writes onto the root field the defaults it was reading off the source's
/// `/AcroForm` — the same materialization [`crate::page_graph`] does for a
/// page's inherited attributes, one level up, and for the same reason: the
/// dictionary they were inherited from is not coming along.
pub(crate) fn inherit_form_defaults(
    doc: &mut LopdfRawDocument,
    root: ObjectId,
    donor_form: &Dictionary,
) {
    for key in [b"DA".as_slice(), b"Q".as_slice()] {
        let Ok(value) = donor_form.get(key) else {
            continue;
        };
        if let Ok(dict) = doc.get_dictionary_mut(root) {
            if dict.get(key).is_err() {
                dict.set(key.to_vec(), value.clone());
            }
        }
    }
}
