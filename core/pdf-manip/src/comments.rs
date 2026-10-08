//! Read-only comment snapshots. Existing annotations stay in the PDF object
//! graph: adding them to the pending annotation set would write them twice.

use lopdf::{Dictionary, Object};
use pdf_document::Rect;

use crate::LopdfDocument;

/// A comment anchored in unrotated PDF page coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfComment {
    pub rect: Rect,
    pub contents: String,
    pub author: Option<String>,
    /// Original PDF date string, without guessing a timezone or precision.
    pub date: Option<String>,
}

impl LopdfDocument {
    /// Reads all pages in one page-tree traversal for a document-wide panel.
    pub fn comments(&self) -> Vec<Vec<PdfComment>> {
        self.0
            .get_pages()
            .into_values()
            .map(|id| self.comments_on_page(id))
            .collect()
    }

    fn comments_on_page(&self, page_id: lopdf::ObjectId) -> Vec<PdfComment> {
        let doc = &self.0;
        let Some(annots) = doc
            .get_dictionary(page_id)
            .ok()
            .and_then(|page| page.get(b"Annots").ok())
            .and_then(|value| resolve(doc, value))
            .and_then(|value| value.as_array().ok())
        else {
            return Vec::new();
        };
        annots
            .iter()
            .filter_map(|value| {
                let dict = resolve(doc, value)?.as_dict().ok()?;
                let subtype = resolve(doc, dict.get(b"Subtype").ok()?)?.as_name().ok()?;
                if !matches!(
                    subtype,
                    b"Text"
                        | b"FreeText"
                        | b"Line"
                        | b"Square"
                        | b"Circle"
                        | b"Polygon"
                        | b"PolyLine"
                        | b"Highlight"
                        | b"Underline"
                        | b"Squiggly"
                        | b"StrikeOut"
                        | b"Stamp"
                        | b"Caret"
                        | b"Ink"
                        | b"FileAttachment"
                        | b"Sound"
                        | b"Redact"
                ) {
                    return None;
                }
                let contents = text(doc, dict, b"Contents")?;
                if contents.trim().is_empty() {
                    return None;
                }
                let coords = resolve(doc, dict.get(b"Rect").ok()?)?.as_array().ok()?;
                if coords.len() != 4 {
                    return None;
                }
                let mut numbers = [0.0; 4];
                for (number, value) in numbers.iter_mut().zip(coords) {
                    *number = match resolve(doc, value)? {
                        Object::Integer(n) => *n as f64,
                        Object::Real(n) => f64::from(*n),
                        _ => return None,
                    };
                    if !number.is_finite() {
                        return None;
                    }
                }
                let [x0, y0, x1, y1] = numbers;
                Some(PdfComment {
                    rect: Rect {
                        x: x0.min(x1),
                        y: y0.min(y1),
                        width: (x1 - x0).abs(),
                        height: (y1 - y0).abs(),
                    },
                    contents,
                    author: text(doc, dict, b"T"),
                    date: text(doc, dict, b"M").or_else(|| text(doc, dict, b"CreationDate")),
                })
            })
            .collect()
    }
}

fn resolve<'a>(doc: &'a lopdf::Document, mut value: &'a Object) -> Option<&'a Object> {
    for _ in 0..64 {
        match value {
            Object::Reference(id) => value = doc.objects.get(id)?,
            _ => return Some(value),
        }
    }
    None
}

fn text(doc: &lopdf::Document, dict: &Dictionary, key: &[u8]) -> Option<String> {
    let value = resolve(doc, dict.get(key).ok()?)?;
    // lopdf 0.45 retains the UTF-8 BOM in its decoded string.
    if let Some(bytes) = value.as_str().ok()?.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(bytes.to_vec()).ok();
    }
    lopdf::decode_text_string(value).ok()
}
