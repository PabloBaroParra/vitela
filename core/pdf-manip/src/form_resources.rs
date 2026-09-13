//! The imported form's default resources, and the name collision they bring
//! with them (checklist "Estructuras de documento",
//! `docs/batch-pdf-assembly.md` section 4).
//!
//! A field says how to draw its own text with a `/DA` string — `/Helv 0 Tf
//! 0 g` — and that `/Helv` is a *resource name*, resolved against the form's
//! `/DR`. The name travels with the field; the entry it points at does not,
//! because `/DR` hangs off the `/AcroForm` in the catalog a graft leaves
//! behind. Copy the field alone and its `/DA` resolves to whatever the
//! destination happens to call `/Helv`, or to nothing at all.
//!
//! So the entries are copied over, and any name the destination already uses
//! for something else is rebound to a free one — with the `/DA` strings that
//! depended on it rewritten to follow. Nothing is lost and nothing is
//! user-visible, which is why this collision, unlike a field name's, is not
//! reported.

use std::collections::{BTreeSet, HashMap, HashSet};

use lopdf::{Dictionary, Document as LopdfRawDocument, Object, ObjectId};

use crate::forms::{dereference, dereferenced_dict, IMPORTED_SUFFIX, MAX_FIELD_DEPTH};
use crate::page_graph::collect_reachable;

/// Copies the source form's `/DR` entries into the destination's, rebinding
/// any name the destination already uses for something else.
///
/// Returns the `/Font` rebindings only: `/DA` is the one place a resource
/// name travels *inside a string*, and a font is the only thing it names
/// there (PDF 32000-1:2008 section 12.7.3.3). Every other category is
/// reached by name from an appearance stream's own `/Resources`, which came
/// along with the stream, so renaming one there changes nothing.
pub(crate) fn merge_default_resources(
    doc: &mut LopdfRawDocument,
    donor: &LopdfRawDocument,
    form: &mut Dictionary,
    donor_form: &Dictionary,
) -> HashMap<Vec<u8>, Vec<u8>> {
    let Some(donor_resources) = donor_form
        .get(b"DR")
        .ok()
        .and_then(|value| dereferenced_dict(donor, value))
    else {
        return HashMap::new();
    };
    let mut resources = form
        .get(b"DR")
        .ok()
        .and_then(|value| dereferenced_dict(doc, value))
        .unwrap_or_default();

    let mut rebound = HashMap::new();
    for (category, entries) in donor_resources.iter() {
        let Some(entries) = dereferenced_dict(donor, entries) else {
            continue;
        };
        let mut merged = resources
            .get(category)
            .ok()
            .and_then(|value| dereferenced_dict(doc, value))
            .unwrap_or_default();
        for (name, value) in entries.iter() {
            copy_resource(doc, donor, value);
            let chosen = free_resource_name(doc, donor, &merged, name, value);
            if merged.get(&chosen).is_err() {
                merged.set(chosen.clone(), value.clone());
            }
            if chosen != *name && category == b"Font" {
                rebound.insert(name.clone(), chosen);
            }
        }
        resources.set(category.clone(), merged);
    }
    form.set("DR", resources);
    rebound
}

/// The name the imported resource ends up under: its own when free or already
/// bound to the very same object, and the first free suffixed one otherwise.
fn free_resource_name(
    doc: &LopdfRawDocument,
    donor: &LopdfRawDocument,
    merged: &Dictionary,
    name: &[u8],
    value: &Object,
) -> Vec<u8> {
    let mut candidate = name.to_vec();
    for attempt in 1..MAX_FIELD_DEPTH {
        match merged.get(&candidate) {
            Err(_) => return candidate,
            Ok(existing) if same_resource(doc, donor, existing, value) => return candidate,
            Ok(_) => {}
        }
        candidate = match attempt {
            1 => format!("{}{IMPORTED_SUFFIX}", String::from_utf8_lossy(name)).into_bytes(),
            n => format!("{}{IMPORTED_SUFFIX}-{}", String::from_utf8_lossy(name), n).into_bytes(),
        };
    }
    candidate
}

/// Whether two resource entries mean the same thing, compared by what they
/// resolve to rather than by object id — the id differs on every import, and
/// re-importing the same source twice must not keep adding fonts.
fn same_resource(
    doc: &LopdfRawDocument,
    donor: &LopdfRawDocument,
    existing: &Object,
    imported: &Object,
) -> bool {
    match (dereference(doc, existing), dereference(donor, imported)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn copy_resource(doc: &mut LopdfRawDocument, donor: &LopdfRawDocument, value: &Object) {
    let mut holder = Dictionary::new();
    holder.set("Resource", value.clone());
    let mut reachable = BTreeSet::new();
    collect_reachable(donor, &holder, &HashSet::new(), &mut reachable);
    for id in reachable {
        if let Ok(object) = donor.get_object(id) {
            doc.objects.entry(id).or_insert_with(|| object.clone());
        }
    }
}

/// Rewrites the font resource name inside a `/DA` string so the field keeps
/// pointing at the font it always did.
///
/// `/DA` is a content-stream fragment (`/Helv 0 Tf 0 g`); the name that
/// matters is the operand two tokens before `Tf`.
pub(crate) fn rebind_default_appearance(
    doc: &mut LopdfRawDocument,
    node: ObjectId,
    rebound: &HashMap<Vec<u8>, Vec<u8>>,
) {
    let Ok(dict) = doc.get_dictionary_mut(node) else {
        return;
    };
    let Ok(appearance) = dict.get(b"DA").and_then(|da| da.as_str()) else {
        return;
    };
    let tokens: Vec<&[u8]> = appearance
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|token| !token.is_empty())
        .collect();
    let Some(operator) = tokens.iter().position(|token| *token == b"Tf") else {
        return;
    };
    if operator < 2 {
        return;
    }
    let Some(name) = tokens[operator - 2].strip_prefix(b"/") else {
        return;
    };
    let Some(replacement) = rebound.get(name) else {
        return;
    };
    let mut rewritten = tokens.iter().map(|t| t.to_vec()).collect::<Vec<_>>();
    rewritten[operator - 2] = [b"/".as_slice(), replacement].concat();
    dict.set(
        "DA",
        Object::String(rewritten.join(&b' '), lopdf::StringFormat::Literal),
    );
}
