//! Parses the `/AcroForm` of an already-open `lopdf::Document` into
//! `FormField` values with `origin: Existing(oid)` (T-137).
//!
//! Reading is entirely best-effort: a field, kid, or `/AcroForm` entry this
//! parser cannot make sense of is silently left unmodeled rather than
//! aborting the whole read — "lo no modelado se preserva intacto y queda
//! fuera del set editable" (ficha). `/FT /Sig`, pushbuttons, non-combo
//! choice fields (listboxes), and any field this module cannot classify are
//! exactly that: still present in the file, invisible to this crate.
//!
//! Single-hop reference resolution ([`resolve`]) mirrors
//! `pdf_edit::encoding::resolve` rather than the bounded-chain walk in
//! `pdf-save::bridge::resolve_object` — PDF references are practically
//! always direct, and a reference this shallow lookup cannot follow simply
//! fails the surrounding `Option` chain, which here means "skip this field",
//! not "error the whole read".

use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::HashMap;

use pdf_document::{
    FieldOrigin, FieldValue, FormField, FormFieldId, FormFieldKind, PageId, RadioOption, Rect,
};

use crate::da::parse_da;

// `/Ff` bit numbers, 1-indexed per ISO 32000-1 Tables 227/229/230 — bit N's
// mask is `1 << (N - 1)`.
const FF_TX_MULTILINE: u32 = 13;
const FF_BTN_RADIO: u32 = 16;
const FF_BTN_PUSHBUTTON: u32 = 17;
const FF_CH_COMBO: u32 = 18;
const FF_CH_EDIT: u32 = 19;

fn has_flag(flags: i64, bit: u32) -> bool {
    flags & (1 << (bit - 1)) != 0
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => document.get_object(*id).unwrap_or(object),
        direct => direct,
    }
}

/// Decodes a PDF text string (ISO 32000-2 §7.9.2.2): UTF-16BE when it opens
/// with the `FE FF` byte-order mark, PDFDocEncoding otherwise — approximated
/// as a direct byte-to-codepoint mapping (covers its printable-ASCII range
/// exactly; the handful of typographic marks PDFDocEncoding remaps above
/// 0x80 are not modeled). A field value outside what this crate can later
/// re-encode is caught by `appearance::build_field_appearance`'s own
/// printable-ASCII check when the field is actually rendered, not here —
/// reading a value is not the same as being able to keep editing it.
fn decode_text_string(bytes: &[u8]) -> String {
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

fn rect_of(document: &Document, dict: &Dictionary) -> Option<Rect> {
    let array = resolve(document, dict.get(b"Rect").ok()?).as_array().ok()?;
    if array.len() != 4 {
        return None;
    }
    let coord = |object: &Object| -> Option<f64> {
        match resolve(document, object) {
            Object::Integer(i) => Some(*i as f64),
            Object::Real(r) => Some(*r as f64),
            _ => None,
        }
    };
    let x0 = coord(&array[0])?;
    let y0 = coord(&array[1])?;
    let x1 = coord(&array[2])?;
    let y1 = coord(&array[3])?;
    Some(Rect {
        x: x0.min(x1),
        y: y0.min(y1),
        width: (x1 - x0).abs(),
        height: (y1 - y0).abs(),
    })
}

/// Maps every widget annotation object id to the `PageId` its page will get
/// under this crate's population convention (0-indexed, in `get_pages()`'s
/// own page-number order — the same convention `pdf-save::bridge::
/// populate_document` uses for `Document.pages`, so a field's `page` lines
/// up with the page list a caller already built from the same document).
fn widget_pages(document: &Document) -> HashMap<ObjectId, PageId> {
    let mut map = HashMap::new();
    for (index, page_id) in document.get_pages().values().enumerate() {
        let Ok(page_dict) = document.get_dictionary(*page_id) else {
            continue;
        };
        let Some(annots) = page_dict
            .get(b"Annots")
            .ok()
            .map(|object| resolve(document, object))
            .and_then(|object| object.as_array().ok())
        else {
            continue;
        };
        for annot in annots {
            if let Object::Reference(annot_id) = annot {
                map.insert(*annot_id, PageId(index as u32));
            }
        }
    }
    map
}

/// One export-value/rect option read from a radio group's widget kids, or
/// `None` if any kid lacks a usable `/Rect` or `/AP /N` — a partially
/// modeled radio group (missing one button) is worse than an unmodeled one,
/// so any kid failure drops the whole field.
fn radio_option_of(document: &Document, kid_id: ObjectId) -> Option<RadioOption> {
    let dict = document.get_dictionary(kid_id).ok()?;
    let rect = rect_of(document, dict)?;
    let ap = resolve(document, dict.get(b"AP").ok()?).as_dict().ok()?;
    let normal = resolve(document, ap.get(b"N").ok()?).as_dict().ok()?;
    let export_value = normal
        .iter()
        .map(|(key, _)| key)
        .find(|key| key.as_slice() != b"Off")
        .map(|key| String::from_utf8_lossy(key).to_string())?;
    Some(RadioOption { export_value, rect })
}

/// `true` when `dict`'s own `/Kids` (if any) are widget annotations of this
/// same terminal field — no own `/T`, no own `/FT` — rather than genuinely
/// separate child fields contributing their own name segment. A node with
/// even one named or typed kid is a naming group, not a fillable field
/// itself, and every kid is then walked as a child field instead.
fn kids_are_widgets(document: &Document, kids: &[Object]) -> bool {
    kids.iter().all(|kid| {
        let Object::Reference(id) = kid else {
            return false;
        };
        let Ok(dict) = document.get_dictionary(*id) else {
            return false;
        };
        !dict.has(b"T") && !dict.has(b"FT")
    })
}

fn qualified_name(parent: Option<&str>, own: Option<&str>) -> Option<String> {
    match (parent, own) {
        (Some(parent), Some(own)) => Some(format!("{parent}.{own}")),
        (None, Some(own)) => Some(own.to_string()),
        (Some(parent), None) => Some(parent.to_string()),
        (None, None) => None,
    }
}

fn own_name(dict: &Dictionary) -> Option<String> {
    dict.get(b"T")
        .ok()
        .map(|object| match object {
            Object::String(bytes, _) => decode_text_string(bytes),
            _ => String::new(),
        })
        .filter(|name| !name.is_empty())
}

fn own_ft<'a>(document: &'a Document, dict: &'a Dictionary) -> Option<&'a [u8]> {
    dict.get(b"FT")
        .ok()
        .map(|object| resolve(document, object))
        .and_then(|object| object.as_name().ok())
}

fn flags_of(dict: &Dictionary) -> i64 {
    dict.get(b"Ff")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
}

/// Builds the terminal field at `(id, dict)`, given its resolved `/FT`,
/// `/Ff`, qualified name, and the page each widget-shaped kid (if any)
/// lands on. Returns `None` for a field this crate does not model:
/// `/FT /Sig`, pushbuttons, non-combo choice fields, or one whose sole
/// widget cannot be placed on a page.
fn build_terminal_field(
    document: &Document,
    id: ObjectId,
    dict: &Dictionary,
    ft: &[u8],
    flags: i64,
    name: String,
    pages: &HashMap<ObjectId, PageId>,
) -> Option<FormField> {
    let style = parse_da(&da_of(document, dict));
    let value_object = dict.get(b"V").ok().map(|object| resolve(document, object));

    match ft {
        b"Tx" => {
            let text = match value_object {
                Some(Object::String(bytes, _)) => decode_text_string(bytes),
                _ => String::new(),
            };
            let max_len = dict
                .get(b"MaxLen")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .and_then(|n| u32::try_from(n).ok());
            let rect = rect_of(document, dict)?;
            let page = *pages.get(&id)?;
            Some(FormField {
                id: FormFieldId(0),
                page,
                name,
                rect,
                style,
                value: FieldValue::Text(text),
                kind: FormFieldKind::Text {
                    multiline: has_flag(flags, FF_TX_MULTILINE),
                    max_len,
                },
                origin: FieldOrigin::Existing(id),
            })
        }
        b"Btn" if has_flag(flags, FF_BTN_PUSHBUTTON) => None,
        b"Btn" if has_flag(flags, FF_BTN_RADIO) => {
            let kids = dict.get(b"Kids").ok().map(|o| resolve(document, o));
            let kid_ids: Vec<ObjectId> = match kids.and_then(|o| o.as_array().ok()) {
                Some(array) => array
                    .iter()
                    .filter_map(|o| match o {
                        Object::Reference(id) => Some(*id),
                        _ => None,
                    })
                    .collect(),
                None => Vec::new(),
            };
            if kid_ids.is_empty() {
                return None;
            }
            let options: Option<Vec<RadioOption>> = kid_ids
                .iter()
                .map(|kid_id| radio_option_of(document, *kid_id))
                .collect();
            let options = options?;
            let page = *pages.get(&kid_ids[0])?;
            let rect = radio_group_bbox(&options);
            let selected = match value_object {
                Some(Object::Name(name)) if name.as_slice() != b"Off" => {
                    Some(String::from_utf8_lossy(name).to_string())
                }
                _ => None,
            };
            Some(FormField {
                id: FormFieldId(0),
                page,
                name,
                rect,
                style,
                value: FieldValue::Choice(selected),
                kind: FormFieldKind::RadioGroup { options },
                origin: FieldOrigin::Existing(id),
            })
        }
        b"Btn" => {
            let rect = rect_of(document, dict)?;
            let page = *pages.get(&id)?;
            let checked = !is_off(value_object);
            Some(FormField {
                id: FormFieldId(0),
                page,
                name,
                rect,
                style,
                value: FieldValue::Checked(checked),
                kind: FormFieldKind::Checkbox,
                origin: FieldOrigin::Existing(id),
            })
        }
        b"Ch" if has_flag(flags, FF_CH_COMBO) => {
            let options = dict
                .get(b"Opt")
                .ok()
                .map(|o| resolve(document, o))
                .and_then(|o| o.as_array().ok())
                .map(|array| array.iter().filter_map(opt_display).collect())
                .unwrap_or_default();
            let selected = match value_object {
                Some(Object::String(bytes, _)) if !bytes.is_empty() => {
                    Some(decode_text_string(bytes))
                }
                _ => None,
            };
            let rect = rect_of(document, dict)?;
            let page = *pages.get(&id)?;
            Some(FormField {
                id: FormFieldId(0),
                page,
                name,
                rect,
                style,
                value: FieldValue::Choice(selected),
                kind: FormFieldKind::Dropdown {
                    options,
                    editable: has_flag(flags, FF_CH_EDIT),
                },
                origin: FieldOrigin::Existing(id),
            })
        }
        _ => None,
    }
}

fn is_off(value_object: Option<&Object>) -> bool {
    matches!(value_object, Some(Object::Name(name)) if name.as_slice() == b"Off")
        || value_object.is_none()
}

fn opt_display(entry: &Object) -> Option<String> {
    match entry {
        Object::String(bytes, _) => Some(decode_text_string(bytes)),
        Object::Array(pair) if pair.len() == 2 => match &pair[1] {
            Object::String(bytes, _) => Some(decode_text_string(bytes)),
            _ => None,
        },
        _ => None,
    }
}

fn da_of(document: &Document, dict: &Dictionary) -> String {
    dict.get(b"DA")
        .ok()
        .map(|object| resolve(document, object))
        .and_then(|object| object.as_str().ok())
        .map(|bytes| String::from_utf8_lossy(bytes).to_string())
        .unwrap_or_default()
}

/// A radio group has no single natural `/Rect` of its own — each button
/// carries its own — so `FormField.rect` becomes the bounding box of every
/// option's rect, giving move/resize (T-134) a sensible whole-field extent
/// without inventing per-kid tracking this model does not have.
fn radio_group_bbox(options: &[RadioOption]) -> Rect {
    let x0 = options
        .iter()
        .map(|o| o.rect.x)
        .fold(f64::INFINITY, f64::min);
    let y0 = options
        .iter()
        .map(|o| o.rect.y)
        .fold(f64::INFINITY, f64::min);
    let x1 = options
        .iter()
        .map(|o| o.rect.x + o.rect.width)
        .fold(f64::NEG_INFINITY, f64::max);
    let y1 = options
        .iter()
        .map(|o| o.rect.y + o.rect.height)
        .fold(f64::NEG_INFINITY, f64::max);
    Rect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    }
}

fn walk(
    document: &Document,
    entries: &[Object],
    parent_name: Option<&str>,
    pages: &HashMap<ObjectId, PageId>,
    out: &mut Vec<FormField>,
) {
    for entry in entries {
        let Object::Reference(id) = entry else {
            continue;
        };
        let Ok(dict) = document.get_dictionary(*id) else {
            continue;
        };
        let name = own_name(dict);
        let qualified = qualified_name(parent_name, name.as_deref());

        let kids = dict
            .get(b"Kids")
            .ok()
            .map(|o| resolve(document, o))
            .and_then(|o| o.as_array().ok());

        match kids {
            Some(kids) if !kids_are_widgets(document, kids) => {
                // A naming group: recurse into its kids as child fields,
                // never modeling this node itself.
                walk(document, kids, qualified.as_deref(), pages, out);
            }
            _ => {
                let Some(qualified) = qualified else {
                    continue;
                };
                let Some(ft) = own_ft(document, dict) else {
                    continue;
                };
                let flags = flags_of(dict);
                if let Some(field) =
                    build_terminal_field(document, *id, dict, ft, flags, qualified, pages)
                {
                    out.push(field);
                }
            }
        }
    }
}

/// Reads every field this crate models out of `document`'s `/AcroForm`,
/// assigning `FormFieldId`s sequentially in read order starting at 0 — the
/// same convention `pdf-save::bridge::populate_document` uses for `PageId`.
/// A caller that also creates new fields in the same session must start its
/// own id counter past the highest id returned here (T-140's concern, not
/// this function's).
///
/// Returns an empty `Vec` when there is no `/AcroForm`, no `/Fields`, or
/// either is malformed — never an error, matching this module's read
/// resilience posture.
pub fn read_form_fields(document: &Document) -> Vec<FormField> {
    let mut fields = Vec::new();

    let Ok(catalog) = document
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .and_then(|id| document.get_dictionary(id))
    else {
        return fields;
    };
    let Some(acroform) = catalog
        .get(b"AcroForm")
        .ok()
        .map(|o| resolve(document, o))
        .and_then(|o| o.as_dict().ok())
    else {
        return fields;
    };
    let Some(top_fields) = acroform
        .get(b"Fields")
        .ok()
        .map(|o| resolve(document, o))
        .and_then(|o| o.as_array().ok())
    else {
        return fields;
    };

    let pages = widget_pages(document);
    walk(document, top_fields, None, &pages, &mut fields);

    for (index, field) in fields.iter_mut().enumerate() {
        field.id = FormFieldId(index as u64);
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{content::Content, dictionary, Object, Stream};
    use pdf_document::{FontFamily, TextStyle};

    /// A one-page document with a `/AcroForm` built from raw dictionaries —
    /// mirrors `pdf-save::bridge`'s own `labeled_pdf` test-fixture pattern.
    struct FormFixture {
        doc: Document,
        page_id: ObjectId,
    }

    impl FormFixture {
        fn new() -> Self {
            let mut doc = Document::with_version("1.5");
            let pages_id = doc.new_object_id();
            let content = Content { operations: vec![] };
            let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Annots" => Vec::<Object>::new(),
            });
            doc.objects.insert(
                pages_id,
                Object::Dictionary(dictionary! {
                    "Type" => "Pages",
                    "Kids" => vec![Object::Reference(page_id)],
                    "Count" => 1,
                }),
            );
            let catalog_id = doc.add_object(dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
            });
            doc.trailer.set("Root", catalog_id);
            Self { doc, page_id }
        }

        fn add_annot(&mut self, id: ObjectId) {
            let annots = self
                .doc
                .get_dictionary_mut(self.page_id)
                .unwrap()
                .get_mut(b"Annots")
                .unwrap();
            if let Object::Array(array) = annots {
                array.push(Object::Reference(id));
            }
        }

        fn set_acroform(&mut self, field_ids: Vec<ObjectId>) {
            let acroform = self.doc.add_object(dictionary! {
                "Fields" => field_ids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
            });
            let catalog_id = self
                .doc
                .trailer
                .get(b"Root")
                .unwrap()
                .as_reference()
                .unwrap();
            self.doc
                .get_dictionary_mut(catalog_id)
                .unwrap()
                .set("AcroForm", acroform);
        }
    }

    #[test]
    fn no_acroform_reads_no_fields() {
        let fixture = FormFixture::new();
        assert!(read_form_fields(&fixture.doc).is_empty());
    }

    #[test]
    fn reads_a_simple_text_field() {
        let mut fixture = FormFixture::new();
        let field_id = fixture.doc.add_object(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal("Name"),
            "V" => Object::string_literal("Ada"),
            "Rect" => vec![10.into(), 20.into(), 110.into(), 40.into()],
            "DA" => Object::string_literal("0 0 0 rg /Helv 12 Tf"),
        });
        fixture.add_annot(field_id);
        fixture.set_acroform(vec![field_id]);

        let fields = read_form_fields(&fixture.doc);
        assert_eq!(fields.len(), 1);
        let field = &fields[0];
        assert_eq!(field.name, "Name");
        assert_eq!(field.value, FieldValue::Text("Ada".to_string()));
        assert_eq!(field.page, PageId(0));
        assert_eq!(field.origin, FieldOrigin::Existing(field_id));
        assert_eq!(
            field.rect,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 20.0
            }
        );
        assert_eq!(
            field.style,
            TextStyle {
                font: FontFamily::Helvetica,
                size_pt: 12.0,
                color: pdf_document::Color { r: 0, g: 0, b: 0 },
            }
        );
        assert!(matches!(
            field.kind,
            FormFieldKind::Text {
                multiline: false,
                max_len: None
            }
        ));
    }

    #[test]
    fn multiline_flag_and_max_len_are_read() {
        let mut fixture = FormFixture::new();
        let field_id = fixture.doc.add_object(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal("Notes"),
            "Ff" => 1 << 12, // bit 13, 0-indexed shift
            "MaxLen" => 500,
            "Rect" => vec![0.into(), 0.into(), 200.into(), 100.into()],
        });
        fixture.add_annot(field_id);
        fixture.set_acroform(vec![field_id]);

        let fields = read_form_fields(&fixture.doc);
        match fields[0].kind {
            FormFieldKind::Text { multiline, max_len } => {
                assert!(multiline);
                assert_eq!(max_len, Some(500));
            }
            ref other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn reads_a_checked_checkbox() {
        let mut fixture = FormFixture::new();
        let field_id = fixture.doc.add_object(dictionary! {
            "FT" => "Btn",
            "T" => Object::string_literal("Agree"),
            "V" => Object::Name(b"Yes".to_vec()),
            "Rect" => vec![0.into(), 0.into(), 12.into(), 12.into()],
        });
        fixture.add_annot(field_id);
        fixture.set_acroform(vec![field_id]);

        let fields = read_form_fields(&fixture.doc);
        assert_eq!(fields[0].value, FieldValue::Checked(true));
        assert!(matches!(fields[0].kind, FormFieldKind::Checkbox));
    }

    #[test]
    fn reads_an_unchecked_checkbox_with_no_value() {
        let mut fixture = FormFixture::new();
        let field_id = fixture.doc.add_object(dictionary! {
            "FT" => "Btn",
            "T" => Object::string_literal("Agree"),
            "Rect" => vec![0.into(), 0.into(), 12.into(), 12.into()],
        });
        fixture.add_annot(field_id);
        fixture.set_acroform(vec![field_id]);

        let fields = read_form_fields(&fixture.doc);
        assert_eq!(fields[0].value, FieldValue::Checked(false));
    }

    fn radio_kid(doc: &mut Document, rect: [i64; 4], export_value: &[u8]) -> ObjectId {
        let on_ap = doc.add_object(Stream::new(dictionary! {}, vec![]));
        let off_ap = doc.add_object(Stream::new(dictionary! {}, vec![]));
        let mut normal = Dictionary::new();
        normal.set(export_value.to_vec(), on_ap);
        normal.set("Off", off_ap);
        let mut ap = Dictionary::new();
        ap.set("N", Object::Dictionary(normal));
        let mut widget = Dictionary::new();
        widget.set("Subtype", "Widget");
        widget.set(
            "Rect",
            rect.iter().map(|&v| v.into()).collect::<Vec<Object>>(),
        );
        widget.set("AP", Object::Dictionary(ap));
        doc.add_object(widget)
    }

    #[test]
    fn reads_a_radio_group_with_its_selection() {
        let mut fixture = FormFixture::new();
        let kid_yes = radio_kid(&mut fixture.doc, [0, 0, 12, 12], b"Yes");
        let kid_no = radio_kid(&mut fixture.doc, [20, 0, 32, 12], b"No");
        let field_id = fixture.doc.add_object(dictionary! {
            "FT" => "Btn",
            "T" => Object::string_literal("Choice"),
            "Ff" => 1 << 15, // bit 16, 0-indexed shift: Radio
            "V" => Object::Name(b"Yes".to_vec()),
            "Kids" => vec![Object::Reference(kid_yes), Object::Reference(kid_no)],
        });
        fixture.add_annot(kid_yes);
        fixture.add_annot(kid_no);
        fixture.set_acroform(vec![field_id]);

        let fields = read_form_fields(&fixture.doc);
        assert_eq!(fields.len(), 1);
        let field = &fields[0];
        assert_eq!(field.value, FieldValue::Choice(Some("Yes".to_string())));
        match &field.kind {
            FormFieldKind::RadioGroup { options } => {
                assert_eq!(options.len(), 2);
                assert_eq!(options[0].export_value, "Yes");
                assert_eq!(options[1].export_value, "No");
            }
            other => panic!("expected RadioGroup, got {other:?}"),
        }
    }

    #[test]
    fn reads_a_dropdown_with_display_options() {
        let mut fixture = FormFixture::new();
        let field_id = fixture.doc.add_object(dictionary! {
            "FT" => "Ch",
            "T" => Object::string_literal("Country"),
            "Ff" => 1 << 17, // bit 18, 0-indexed shift: Combo
            "V" => Object::string_literal("Argentina"),
            "Opt" => vec![
                Object::string_literal("Argentina"),
                Object::string_literal("Brazil"),
            ],
            "Rect" => vec![0.into(), 0.into(), 150.into(), 20.into()],
        });
        fixture.add_annot(field_id);
        fixture.set_acroform(vec![field_id]);

        let fields = read_form_fields(&fixture.doc);
        assert_eq!(
            fields[0].value,
            FieldValue::Choice(Some("Argentina".to_string()))
        );
        match &fields[0].kind {
            FormFieldKind::Dropdown { options, editable } => {
                assert_eq!(
                    options,
                    &vec!["Argentina".to_string(), "Brazil".to_string()]
                );
                assert!(!editable);
            }
            other => panic!("expected Dropdown, got {other:?}"),
        }
    }

    #[test]
    fn a_non_combo_choice_field_is_left_unmodeled() {
        let mut fixture = FormFixture::new();
        let field_id = fixture.doc.add_object(dictionary! {
            "FT" => "Ch",
            "T" => Object::string_literal("List"),
            "Rect" => vec![0.into(), 0.into(), 150.into(), 60.into()],
        });
        fixture.add_annot(field_id);
        fixture.set_acroform(vec![field_id]);

        assert!(read_form_fields(&fixture.doc).is_empty());
    }

    #[test]
    fn a_pushbutton_is_left_unmodeled() {
        let mut fixture = FormFixture::new();
        let field_id = fixture.doc.add_object(dictionary! {
            "FT" => "Btn",
            "T" => Object::string_literal("Submit"),
            "Ff" => 1 << 16, // bit 17, 0-indexed shift: Pushbutton
            "Rect" => vec![0.into(), 0.into(), 60.into(), 20.into()],
        });
        fixture.add_annot(field_id);
        fixture.set_acroform(vec![field_id]);

        assert!(read_form_fields(&fixture.doc).is_empty());
    }

    #[test]
    fn a_signature_field_is_left_unmodeled() {
        let mut fixture = FormFixture::new();
        let field_id = fixture.doc.add_object(dictionary! {
            "FT" => "Sig",
            "T" => Object::string_literal("Signature1"),
            "Rect" => vec![0.into(), 0.into(), 200.into(), 60.into()],
        });
        fixture.add_annot(field_id);
        fixture.set_acroform(vec![field_id]);

        assert!(read_form_fields(&fixture.doc).is_empty());
    }

    #[test]
    fn nested_naming_groups_produce_fully_qualified_names() {
        let mut fixture = FormFixture::new();
        let street_id = fixture.doc.add_object(dictionary! {
            "T" => Object::string_literal("street"),
            "FT" => "Tx",
            "Rect" => vec![0.into(), 0.into(), 100.into(), 20.into()],
        });
        let group_id = fixture.doc.add_object(dictionary! {
            "T" => Object::string_literal("address"),
            "Kids" => vec![Object::Reference(street_id)],
        });
        fixture.add_annot(street_id);
        fixture.set_acroform(vec![group_id]);

        let fields = read_form_fields(&fixture.doc);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "address.street");
    }

    #[test]
    fn field_ids_are_assigned_sequentially_in_read_order() {
        let mut fixture = FormFixture::new();
        let first = fixture.doc.add_object(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal("A"),
            "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        });
        let second = fixture.doc.add_object(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal("B"),
            "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        });
        fixture.add_annot(first);
        fixture.add_annot(second);
        fixture.set_acroform(vec![first, second]);

        let fields = read_form_fields(&fixture.doc);
        assert_eq!(fields[0].id, FormFieldId(0));
        assert_eq!(fields[1].id, FormFieldId(1));
    }

    #[test]
    fn a_field_with_no_matching_page_annotation_is_left_unmodeled() {
        let mut fixture = FormFixture::new();
        // Never added via `add_annot` — this field's widget is not on any
        // page's /Annots, so it cannot be placed and must be skipped.
        let field_id = fixture.doc.add_object(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal("Orphan"),
            "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        });
        fixture.set_acroform(vec![field_id]);

        assert!(read_form_fields(&fixture.doc).is_empty());
    }
}
