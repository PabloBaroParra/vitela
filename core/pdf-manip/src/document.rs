//! `LopdfDocument`: the lopdf-backed document handle used across
//! `pdf-manip`'s public API (T-022..T-026).
//!
//! This wraps `lopdf::Document` rather than re-exporting it directly so that
//! `pdf-manip`'s own callers depend on a type this crate owns — matching
//! design.md's "callers never see lopdf types" intent. `pdf_document` itself
//! never depends on lopdf at all (see crate-level docs); this wrapper is
//! `pdf-manip`'s own boundary, one layer further out.

use lopdf::{Object, ObjectId};

/// Opaque handle wrapping an in-memory `lopdf::Document`.
///
/// Used by every public manipulation function rather than leaking
/// `lopdf::Document` through this crate's API.
#[derive(Debug, Clone)]
pub struct LopdfDocument(pub(crate) lopdf::Document);

/// One page's layout size in PDF points, as a viewer should present it:
/// a `/Rotate` of 90 or 270 swaps the reported axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageDimensions {
    pub width_pt: f64,
    pub height_pt: f64,
}

/// The subset of the `/Info` dictionary a document-properties display wants.
/// Each field is `None` when the dictionary is absent, the key is missing,
/// or the value could not be read as a PDF text string — never guessed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
}

impl LopdfDocument {
    /// Wraps an existing `lopdf::Document`. Exposed (rather than
    /// crate-private) so sibling crates (e.g. the future `pdf-save`, Batch 6)
    /// and this crate's own integration tests can construct/inspect handles
    /// directly without duplicating lopdf-level document construction.
    pub fn from_lopdf(document: lopdf::Document) -> Self {
        Self(document)
    }

    /// Unwraps into the underlying `lopdf::Document`.
    pub fn into_lopdf(self) -> lopdf::Document {
        self.0
    }

    /// Borrows the underlying `lopdf::Document`.
    pub fn as_lopdf(&self) -> &lopdf::Document {
        &self.0
    }

    /// Mutably borrows the underlying `lopdf::Document`.
    pub fn as_lopdf_mut(&mut self) -> &mut lopdf::Document {
        &mut self.0
    }

    /// Number of pages currently in the document.
    pub fn page_count(&self) -> usize {
        self.0.get_pages().len()
    }

    /// Per-page layout dimensions in PDF points, in page order.
    ///
    /// `/MediaBox` and `/Rotate` are resolved with page-tree inheritance
    /// (walking `/Parent`, bounded against reference cycles). A page whose
    /// media box is missing or malformed reports US Letter (612 x 792)
    /// instead of failing: these values size viewer placeholders, and
    /// rendering remains the ground truth for what a page looks like.
    pub fn page_dimensions(&self) -> Vec<PageDimensions> {
        self.0
            .get_pages()
            .values()
            .map(|&page_id| page_dimensions_for(&self.0, page_id))
            .collect()
    }

    /// Reads the trailer's `/Info` dictionary for a document-properties
    /// display. Missing dictionary, missing keys, and unreadable values all
    /// fall back to `None` on their own field rather than failing the whole
    /// read — a document with a `/Title` but no `/Author` should still show
    /// the title it has.
    pub fn info(&self) -> DocumentInfo {
        let Some(dict) = self.info_dictionary() else {
            return DocumentInfo::default();
        };
        DocumentInfo {
            title: self.info_field(dict, b"Title"),
            author: self.info_field(dict, b"Author"),
            creator: self.info_field(dict, b"Creator"),
            producer: self.info_field(dict, b"Producer"),
        }
    }

    fn info_dictionary(&self) -> Option<&lopdf::Dictionary> {
        let info_id = self.0.trailer.get(b"Info").ok()?.as_reference().ok()?;
        self.0.get_dictionary(info_id).ok()
    }

    fn info_field(&self, dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
        let bytes = resolve(&self.0, dict.get(key).ok()?).as_str().ok()?;
        (!bytes.is_empty()).then(|| decode_pdf_text_string(bytes))
    }

    /// Reads the trailer's `/Info` dictionary as `pdf_document`'s full
    /// Batch-22 model (T-169) — the seven standard text keys plus the two
    /// dates, vs. [`Self::info`]'s narrower four-field `DocumentInfo` (no
    /// subject/keywords/dates) that already backs the Linux shell's
    /// read-only properties display. Neither replaces the other: `info`
    /// stays as-is for that display, this is what T-176's editable
    /// metadata panel will read from and, via `Command::SetDocumentInfo`
    /// (T-168), write back through.
    ///
    /// Not cached on `LopdfDocument` — read on demand, same lazy-read
    /// criterion `pdf-edit::read_page_content` (T-149) uses for page
    /// content: the majority of sessions never open the metadata panel, so
    /// there is nothing to keep in sync if the trailer's `/Info` reference
    /// changes underneath a cached copy.
    ///
    /// A date string that fails [`pdf_document::PdfDate::parse`] reports
    /// `None` for that field rather than failing the whole read — same
    /// "never guessed" fallback `info_field` already uses for text.
    pub fn document_info(&self) -> pdf_document::DocumentInfo {
        let Some(dict) = self.info_dictionary() else {
            return pdf_document::DocumentInfo::default();
        };
        pdf_document::DocumentInfo {
            title: self.info_field(dict, b"Title"),
            author: self.info_field(dict, b"Author"),
            subject: self.info_field(dict, b"Subject"),
            keywords: self.info_field(dict, b"Keywords"),
            creator: self.info_field(dict, b"Creator"),
            producer: self.info_field(dict, b"Producer"),
            creation_date: self.info_date_field(dict, b"CreationDate"),
            mod_date: self.info_date_field(dict, b"ModDate"),
        }
    }

    fn info_date_field(
        &self,
        dict: &lopdf::Dictionary,
        key: &[u8],
    ) -> Option<pdf_document::PdfDate> {
        pdf_document::PdfDate::parse(&self.info_field(dict, key)?).ok()
    }
}

/// Decodes a PDF text string (ISO 32000-2 §7.9.2.2): UTF-16BE when the bytes
/// open with the `FE FF` byte-order mark, PDFDocEncoding otherwise.
///
/// The PDFDocEncoding branch only really covers its printable-ASCII range
/// (mapping each byte straight to the matching codepoint) rather than the
/// handful of typographic marks the encoding remaps above 0x80 — every
/// `/Info` string this codebase has produced or seen in the wild stays in
/// that range, and a properties display asks for "good enough to read", not
/// a spec-complete text-string decoder.
fn decode_pdf_text_string(bytes: &[u8]) -> String {
    match bytes.strip_prefix(&[0xFE, 0xFF]) {
        Some(utf16_be) => {
            let units = utf16_be
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_be_bytes(*pair));
            char::decode_utf16(units)
                .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect()
        }
        None => bytes.iter().map(|&byte| byte as char).collect(),
    }
}

const DEFAULT_MEDIA_BOX_PT: (f64, f64) = (612.0, 792.0);
const INHERITANCE_DEPTH_LIMIT: usize = 64;

fn page_dimensions_for(doc: &lopdf::Document, page_id: ObjectId) -> PageDimensions {
    let (width_pt, height_pt) = media_box_size(doc, page_id).unwrap_or(DEFAULT_MEDIA_BOX_PT);
    let rotation = inherited_attribute(doc, page_id, b"Rotate")
        .and_then(|object| resolve(doc, object).as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    if rotation == 90 || rotation == 270 {
        PageDimensions {
            width_pt: height_pt,
            height_pt: width_pt,
        }
    } else {
        PageDimensions {
            width_pt,
            height_pt,
        }
    }
}

fn media_box_size(doc: &lopdf::Document, page_id: ObjectId) -> Option<(f64, f64)> {
    let array = resolve(doc, inherited_attribute(doc, page_id, b"MediaBox")?)
        .as_array()
        .ok()?;
    if array.len() != 4 {
        return None;
    }
    let mut edges = [0.0_f64; 4];
    for (edge, entry) in edges.iter_mut().zip(array) {
        *edge = number(resolve(doc, entry))?;
    }
    let width = (edges[2] - edges[0]).abs();
    let height = (edges[3] - edges[1]).abs();
    (width > 0.0 && height > 0.0).then_some((width, height))
}

/// Finds `key` on the page dictionary or, per the PDF page-tree inheritance
/// rules, on the nearest ancestor `/Parent` that defines it.
fn inherited_attribute<'a>(
    doc: &'a lopdf::Document,
    page_id: ObjectId,
    key: &[u8],
) -> Option<&'a Object> {
    let mut current = page_id;
    for _ in 0..INHERITANCE_DEPTH_LIMIT {
        let dictionary = doc.get_dictionary(current).ok()?;
        if let Ok(value) = dictionary.get(key) {
            return Some(value);
        }
        current = dictionary
            .get(b"Parent")
            .and_then(Object::as_reference)
            .ok()?;
    }
    None
}

fn resolve<'a>(doc: &'a lopdf::Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(object),
        other => other,
    }
}

fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some((*value).into()),
        _ => None,
    }
}

/// Width/height in PDF points for `size`, swapped for `Landscape`
/// orientation. Shared by `create_blank_document` and `insert_blank_page`.
pub(crate) fn oriented_dimensions(
    size: pdf_document::PageSize,
    orientation: pdf_document::Orientation,
) -> (f64, f64) {
    let (width, height) = size.dimensions_pt();
    match orientation {
        pdf_document::Orientation::Portrait => (width, height),
        pdf_document::Orientation::Landscape => (height, width),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{Orientation, PageSize};

    const A4_PT: PageDimensions = PageDimensions {
        width_pt: 595.0,
        height_pt: 842.0,
    };

    fn two_page_a4() -> LopdfDocument {
        let blank = crate::create_blank_document(PageSize::A4, Orientation::Portrait);
        let one = crate::insert_blank_page(&blank, 0, PageSize::A4, Orientation::Portrait)
            .expect("first page should insert");
        crate::insert_blank_page(&one, 1, PageSize::A4, Orientation::Portrait)
            .expect("second page should insert")
    }

    fn page_ids(document: &LopdfDocument) -> Vec<ObjectId> {
        document.0.get_pages().values().copied().collect()
    }

    #[test]
    fn page_dimensions_reads_each_page_media_box() {
        let document = two_page_a4();

        assert_eq!(document.page_dimensions(), vec![A4_PT; 2]);
    }

    #[test]
    fn page_dimensions_falls_back_to_the_inherited_media_box() {
        let mut document = two_page_a4();
        for id in page_ids(&document) {
            document
                .0
                .get_dictionary_mut(id)
                .expect("page dictionary should exist")
                .remove(b"MediaBox");
        }

        assert_eq!(document.page_dimensions(), vec![A4_PT; 2]);
    }

    #[test]
    fn page_dimensions_swaps_axes_for_rotated_pages() {
        let mut document = two_page_a4();
        let first = page_ids(&document)[0];
        document
            .0
            .get_dictionary_mut(first)
            .expect("page dictionary should exist")
            .set("Rotate", 270);

        assert_eq!(
            document.page_dimensions(),
            vec![
                PageDimensions {
                    width_pt: 842.0,
                    height_pt: 595.0,
                },
                A4_PT,
            ]
        );
    }

    #[test]
    fn page_dimensions_uses_the_media_box_extent_not_its_corners() {
        let mut document = two_page_a4();
        let first = page_ids(&document)[0];
        document
            .0
            .get_dictionary_mut(first)
            .expect("page dictionary should exist")
            .set(
                "MediaBox",
                vec![
                    Object::Integer(10),
                    Object::Integer(20),
                    Object::Integer(310),
                    Object::Integer(420),
                ],
            );

        assert_eq!(
            document.page_dimensions(),
            vec![
                PageDimensions {
                    width_pt: 300.0,
                    height_pt: 400.0,
                },
                A4_PT,
            ]
        );
    }

    #[test]
    fn page_dimensions_defaults_to_letter_when_no_media_box_exists() {
        let mut document = two_page_a4();
        let mut all_ids = page_ids(&document);
        all_ids.push(
            crate::create_blank::root_pages_id(&document.0).expect("page tree root should exist"),
        );
        for id in all_ids {
            document
                .0
                .get_dictionary_mut(id)
                .expect("dictionary should exist")
                .remove(b"MediaBox");
        }

        assert_eq!(
            document.page_dimensions(),
            vec![
                PageDimensions {
                    width_pt: 612.0,
                    height_pt: 792.0,
                };
                2
            ]
        );
    }

    fn document_with_info(entries: &[(&str, Object)]) -> LopdfDocument {
        let mut document = two_page_a4();
        let mut info = lopdf::Dictionary::new();
        for (key, value) in entries {
            info.set(*key, value.clone());
        }
        let info_id = document.0.add_object(Object::Dictionary(info));
        document.0.trailer.set("Info", info_id);
        document
    }

    #[test]
    fn info_is_empty_without_a_trailer_info_entry() {
        let document = two_page_a4();

        assert_eq!(document.info(), DocumentInfo::default());
    }

    #[test]
    fn info_decodes_ascii_literal_strings() {
        let document = document_with_info(&[
            (
                "Title",
                Object::string_literal("Contrato de servicios".to_string()),
            ),
            (
                "Author",
                Object::string_literal("Vitela Software".to_string()),
            ),
        ]);

        let info = document.info();
        assert_eq!(info.title.as_deref(), Some("Contrato de servicios"));
        assert_eq!(info.author.as_deref(), Some("Vitela Software"));
        assert_eq!(info.creator, None);
        assert_eq!(info.producer, None);
    }

    #[test]
    fn info_decodes_utf16_be_strings_with_a_byte_order_mark() {
        let mut utf16_be = vec![0xFE, 0xFF];
        for unit in "Café".encode_utf16() {
            utf16_be.extend_from_slice(&unit.to_be_bytes());
        }
        let document = document_with_info(&[("Producer", Object::string_literal(utf16_be))]);

        assert_eq!(document.info().producer.as_deref(), Some("Café"));
    }

    #[test]
    fn info_treats_an_empty_string_the_same_as_a_missing_key() {
        let document = document_with_info(&[("Title", Object::string_literal(Vec::new()))]);

        assert_eq!(document.info().title, None);
    }

    // --- LopdfDocument::document_info (T-169) ------------------------------

    #[test]
    fn document_info_is_the_default_without_a_trailer_info_entry() {
        let document = two_page_a4();

        assert_eq!(
            document.document_info(),
            pdf_document::DocumentInfo::default()
        );
    }

    #[test]
    fn document_info_reads_all_seven_text_fields() {
        let document = document_with_info(&[
            ("Title", Object::string_literal("Contrato".to_string())),
            ("Author", Object::string_literal("Ada".to_string())),
            ("Subject", Object::string_literal("Servicios".to_string())),
            (
                "Keywords",
                Object::string_literal("pdf, contrato".to_string()),
            ),
            (
                "Creator",
                Object::string_literal("pdf-editor-mvp".to_string()),
            ),
            ("Producer", Object::string_literal("pdf-save".to_string())),
        ]);

        let info = document.document_info();
        assert_eq!(info.title.as_deref(), Some("Contrato"));
        assert_eq!(info.author.as_deref(), Some("Ada"));
        assert_eq!(info.subject.as_deref(), Some("Servicios"));
        assert_eq!(info.keywords.as_deref(), Some("pdf, contrato"));
        assert_eq!(info.creator.as_deref(), Some("pdf-editor-mvp"));
        assert_eq!(info.producer.as_deref(), Some("pdf-save"));
    }

    #[test]
    fn document_info_parses_creation_and_mod_dates() {
        let document = document_with_info(&[
            (
                "CreationDate",
                Object::string_literal("D:20260831120000Z".to_string()),
            ),
            (
                "ModDate",
                Object::string_literal("D:20260901083000+05'30'".to_string()),
            ),
        ]);

        let info = document.document_info();
        assert_eq!(
            info.creation_date,
            Some(pdf_document::PdfDate::parse("D:20260831120000Z").unwrap())
        );
        assert_eq!(
            info.mod_date,
            Some(pdf_document::PdfDate::parse("D:20260901083000+05'30'").unwrap())
        );
    }

    /// Malformed date bytes fall back to `None` on that field alone, same
    /// "never guessed" behavior `info_field` already uses for unreadable
    /// text — the rest of the dictionary must still come through.
    #[test]
    fn document_info_reports_none_for_an_unparseable_date_without_failing_the_whole_read() {
        let document = document_with_info(&[
            ("Title", Object::string_literal("Contrato".to_string())),
            (
                "CreationDate",
                Object::string_literal("not a pdf date".to_string()),
            ),
        ]);

        let info = document.document_info();
        assert_eq!(info.title.as_deref(), Some("Contrato"));
        assert_eq!(info.creation_date, None);
    }

    #[test]
    fn document_info_decodes_utf16_be_title() {
        let mut utf16_be = vec![0xFE, 0xFF];
        for unit in "Título".encode_utf16() {
            utf16_be.extend_from_slice(&unit.to_be_bytes());
        }
        let document = document_with_info(&[("Title", Object::string_literal(utf16_be))]);

        assert_eq!(document.document_info().title.as_deref(), Some("Título"));
    }
}
