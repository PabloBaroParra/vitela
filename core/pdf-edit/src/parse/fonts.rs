//! The fonts a page declares, by resource name.
//!
//! Split out of [`super`] rather than left beside the content parse: this
//! answers a question *about* a page's resources, not about what it paints,
//! and the two share only the resource lookup they both start from.

use std::collections::BTreeMap;

use lopdf::{Document, Object, ObjectId};
use pdf_document::PageId;

use super::{dereference, page_object_id, page_resources};
use crate::error::EditError;

/// The `/BaseFont` name of every font a page's resources declare, keyed by
/// the resource name a [`pdf_document::TextRun`] reports
/// (`resource_font_name`).
///
/// Exists for shells that draw their own editing overlay on top of the page.
/// A run says which resource it is painted with, not what that resource *is*,
/// so an overlay has no way to pick a face that matches the page — and a
/// mismatched face lands in the wrong place, at the wrong width, however
/// carefully it is positioned. The name is returned raw, subset prefix and
/// style suffix included (`ABCDEF+Times-Bold`): trimming it is a decision
/// about which local font to substitute, which belongs to whoever is doing
/// the substituting.
///
/// A font whose dictionary has no readable `/BaseFont` is left out rather
/// than guessed at, so a caller can tell "the page did not say" from "the
/// page said something I do not recognise".
pub fn page_font_families(
    document: &Document,
    page: PageId,
) -> Result<BTreeMap<String, String>, EditError> {
    let page_object = page_object_id(document, page)?;
    page_object_font_families(document, page_object)
}

/// The object-resolved twin of [`page_font_families`], for the same reason
/// [`read_page_object_content`] exists: an imported page has no position in
/// the document being edited, so the overlay that wants to match its fonts
/// cannot ask for them by one.
pub fn page_object_font_families(
    document: &Document,
    page_object: ObjectId,
) -> Result<BTreeMap<String, String>, EditError> {
    let page_dict = document.get_dictionary(page_object)?;
    let resources = page_resources(document, page_dict);

    let mut families = BTreeMap::new();
    let Some(fonts) = dereference(document, resources.get(b"Font").unwrap_or(&Object::Null)) else {
        return Ok(families);
    };
    let Object::Dictionary(fonts) = fonts else {
        return Ok(families);
    };

    for (name, value) in fonts.iter() {
        let Some(Object::Dictionary(font_dict)) = dereference(document, value) else {
            continue;
        };
        let Some(base_font) = dereference(
            document,
            font_dict.get(b"BaseFont").unwrap_or(&Object::Null),
        )
        .and_then(|object| object.as_name().ok()) else {
            continue;
        };
        families.insert(
            String::from_utf8_lossy(name).into_owned(),
            String::from_utf8_lossy(base_font).into_owned(),
        );
    }

    Ok(families)
}
