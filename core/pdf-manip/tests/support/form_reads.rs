//! Reading a form back out of a document, for the assertions in the graft
//! form tests.
//!
//! Kept apart from [`super::forms`]'s builders for the reason [`super`]'s own
//! split states: a fixture that builds a form and a helper that reads one are
//! two different jobs, and only one of them may be wrong when a test fails.

use lopdf::{Dictionary, Object};
use pdf_manip::LopdfDocument;

/// The document's `/AcroForm`, resolved through an indirect reference if that
/// is how it is stored.
pub fn acroform(document: &LopdfDocument) -> Dictionary {
    let doc = document.as_lopdf();
    let form = doc
        .catalog()
        .expect("the document has a catalog")
        .get(b"AcroForm")
        .expect("the document has an /AcroForm");
    match form {
        Object::Reference(id) => doc
            .get_dictionary(*id)
            .expect("/AcroForm resolves to a dictionary")
            .clone(),
        other => other
            .as_dict()
            .expect("/AcroForm is a dictionary")
            .to_owned(),
    }
}

pub fn has_acroform(document: &LopdfDocument) -> bool {
    document
        .as_lopdf()
        .catalog()
        .map(|catalog| catalog.get(b"AcroForm").is_ok())
        .unwrap_or(false)
}

/// Every top-level field in `/AcroForm /Fields`, resolved to its dictionary.
pub fn form_fields(document: &LopdfDocument) -> Vec<Dictionary> {
    let doc = document.as_lopdf();
    acroform(document)
        .get(b"Fields")
        .and_then(|fields| fields.as_array())
        .expect("/AcroForm has a /Fields array")
        .iter()
        .map(|field| {
            let id = field.as_reference().expect("a field is an indirect object");
            doc.get_dictionary(id)
                .expect("a field resolves to a dictionary")
                .clone()
        })
        .collect()
}

/// The `/T` of every top-level field, in order.
pub fn form_field_names(document: &LopdfDocument) -> Vec<String> {
    form_fields(document)
        .iter()
        .map(|field| text(field, b"T"))
        .collect()
}

/// A string-valued entry of `dict`, or the empty string when it is absent.
pub fn text(dict: &Dictionary, key: &[u8]) -> String {
    dict.get(key)
        .and_then(|value| value.as_str())
        .map(|bytes| String::from_utf8_lossy(bytes).to_string())
        .unwrap_or_default()
}

/// The field named `name`, or `None` when no top-level field has that `/T`.
pub fn form_field(document: &LopdfDocument, name: &str) -> Option<Dictionary> {
    form_fields(document)
        .into_iter()
        .find(|field| text(field, b"T") == name)
}

/// The `/BaseFont` each `/DR /Font` entry resolves to, keyed by resource name
/// — the mapping an imported `/DA` actually depends on.
pub fn default_resource_fonts(document: &LopdfDocument) -> Vec<(String, String)> {
    let doc = document.as_lopdf();
    let form = acroform(document);
    let Ok(resources) = form.get(b"DR").and_then(|dr| dr.as_dict()) else {
        return Vec::new();
    };
    let Ok(fonts) = resources.get(b"Font").and_then(|fonts| fonts.as_dict()) else {
        return Vec::new();
    };
    let mut named: Vec<(String, String)> = fonts
        .iter()
        .map(|(name, value)| {
            let id = value.as_reference().expect("a /DR font is indirect");
            let base = doc
                .get_dictionary(id)
                .expect("a /DR font resolves to a dictionary")
                .get(b"BaseFont")
                .and_then(|base| base.as_name())
                .map(|base| String::from_utf8_lossy(base).to_string())
                .unwrap_or_default();
            (String::from_utf8_lossy(name).to_string(), base)
        })
        .collect();
    named.sort();
    named
}

/// Every `/Subtype /Widget` annotation on the 0-based page `page`.
pub fn page_widgets(document: &LopdfDocument, page: usize) -> Vec<Dictionary> {
    let doc = document.as_lopdf();
    let page_id = doc
        .get_pages()
        .into_values()
        .nth(page)
        .expect("the document has that page");
    let Ok(annots) = doc
        .get_dictionary(page_id)
        .expect("the page resolves")
        .get(b"Annots")
        .and_then(|annots| annots.as_array())
    else {
        return Vec::new();
    };
    annots
        .iter()
        .filter_map(|annot| doc.get_dictionary(annot.as_reference().ok()?).ok())
        .filter(|annot| annot.get(b"Subtype").and_then(|s| s.as_name()).ok() == Some(b"Widget"))
        .cloned()
        .collect()
}

/// The `/V` of the top-level field named `name`.
pub fn field_value(document: &LopdfDocument, name: &str) -> String {
    text(
        &form_field(document, name).unwrap_or_else(|| panic!("no field called {name}")),
        b"V",
    )
}
