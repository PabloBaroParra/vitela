//! Applies `Command::SetDocumentInfo` to the `/Info` dict at save time
//! (T-170/T-171, Batch 22 — see `docs/batch-metadata-edit.md`).
//!
//! Generic over [`ObjectSink`], like [`crate::forms`] and
//! [`crate::annotations`]: unlike page-content edits (Batch 21 decision 5,
//! always a full rewrite), decision 8 has the incremental writer gain
//! metadata support rather than being routed around it, so
//! `apply_document_info` has to work against both
//! `lopdf::Document` (full rewrite) and `lopdf::IncrementalDocument`
//! (incremental) from the start. `ObjectSink::page_dict_mut` already clones
//! an object into the incremental writer's `new_document` before handing out
//! a mutable reference — reusing it for the `/Info` dict's own object id is
//! what closes `strategy.rs`'s former clone-before-mutate deferral (T-171),
//! with no extra code of its own.

use lopdf::{Dictionary, Object};
use pdf_document::{Command, Document, DocumentInfo, PdfDate};

use crate::annotations::ObjectSink;
use crate::error::SaveError;

/// The most recent `SetDocumentInfo.after` still in `document`'s edit log, if
/// any. Batch decision 5 edits `/Info` as a whole from one panel, so a
/// `SetDocumentInfo` command is a complete snapshot, not a diff — the last
/// one recorded is the only one that matters; an earlier one in the same log
/// is already superseded by it. `entries()` (not the redo stack) is exactly
/// "what's currently applied", so an undone edit correctly drops out.
pub fn pending_document_info(document: &Document) -> Option<&DocumentInfo> {
    document
        .pending_edits
        .entries()
        .iter()
        .rev()
        .find_map(|command| match command {
            Command::SetDocumentInfo { after, .. } => Some(after),
            _ => None,
        })
}

/// Applies `info` to `sink`'s `/Info` dictionary, creating the dictionary
/// (and pointing the trailer at it) if the file did not already have one.
///
/// Every field follows batch decision 3: `None` removes the key, `Some`
/// writes it — including `Some(String::new())`, which is a deliberately
/// different, if unusual, state from `None` (an `/Info` key present with an
/// empty value). Translating "the user cleared this field in the UI" into
/// `None` before recording the command is `Command::SetDocumentInfo`'s
/// caller's job (T-176), not this function's.
///
/// Does not touch `/ModDate` specially — the full-rewrite caller is
/// responsible for decision 6's precedence (an explicit `info.mod_date` must
/// win over `set_mod_date`'s auto-stamp for this save); see
/// [`crate::strategy::save_full_rewrite`]. The incremental writer never
/// auto-stamps `/ModDate` at all (a pre-existing, unrelated fact this
/// function does not change), so no such precedence question arises there —
/// see [`crate::strategy::save_incremental`].
pub fn apply_document_info<S: ObjectSink>(
    sink: &mut S,
    info: &DocumentInfo,
) -> Result<(), SaveError> {
    let dict = info_dict_mut(sink)?;
    set_text_field(dict, "Title", &info.title);
    set_text_field(dict, "Author", &info.author);
    set_text_field(dict, "Subject", &info.subject);
    set_text_field(dict, "Keywords", &info.keywords);
    set_text_field(dict, "Creator", &info.creator);
    set_text_field(dict, "Producer", &info.producer);
    set_date_field(dict, "CreationDate", info.creation_date);
    set_date_field(dict, "ModDate", info.mod_date);
    Ok(())
}

/// Resolves the trailer's `/Info` dictionary, creating an empty one (and
/// pointing the trailer at it) if the file has none yet — same fallback
/// `set_mod_date` already uses when it needs to write `/ModDate` into a file
/// with no prior `/Info`. On the incremental writer, `sink.page_dict_mut`
/// clones the dictionary into `new_document` before handing back a mutable
/// reference — the same clone-before-mutate `add_xobject` and this crate's
/// own `write_form_fields` already rely on for other trailer-reachable
/// dicts, which is what makes editing `/Info` on an incremental save safe.
fn info_dict_mut<S: ObjectSink>(sink: &mut S) -> Result<&mut Dictionary, SaveError> {
    let info_ref = sink
        .trailer()
        .get(b"Info")
        .ok()
        .and_then(|object| object.as_reference().ok());
    let info_id = match info_ref {
        Some(id) => id,
        None => {
            let id = sink.add_object(Object::Dictionary(Dictionary::new()));
            sink.trailer_mut().set("Info", id);
            id
        }
    };
    sink.page_dict_mut(info_id)
}

fn set_text_field(dict: &mut Dictionary, key: &str, value: &Option<String>) {
    match value {
        Some(text) => dict.set(
            key,
            Object::String(encode_pdf_text_string(text), lopdf::StringFormat::Literal),
        ),
        None => {
            dict.remove(key.as_bytes());
        }
    }
}

fn set_date_field(dict: &mut Dictionary, key: &str, value: Option<PdfDate>) {
    match value {
        Some(date) => dict.set(key, Object::string_literal(date.to_pdf_string())),
        None => {
            dict.remove(key.as_bytes());
        }
    }
}

/// Encodes a PDF text string per ISO 32000-2 §7.9.2.2 (batch decision 7):
/// PDFDocEncoding when the text fits, UTF-16BE with a leading `FE FF`
/// byte-order mark otherwise — the same choice `pdf-form::appearance` faces
/// for field values, except decision 7 explicitly rules out ever *rejecting*
/// an edit, so unlike that call site there is no error path here: UTF-16BE
/// covers all of Unicode, so every `String` a user can type is
/// representable one way or the other.
///
/// "Fits" is deliberately conservative — printable ASCII (0x20-0x7E) only,
/// the exact range `pdf_manip::document`'s own decoder documents as the part
/// of PDFDocEncoding it (and this function) treats as unambiguous. Real
/// PDFDocEncoding also covers most of Latin-1's upper half, but remaps
/// 0x18-0x1F and 0x80-0x9F to typographic marks Latin-1 does not have at
/// those code points — writing a Latin-1 byte there on the assumption that
/// PDFDocEncoding agrees would silently corrupt the text on any reader that
/// implements the encoding correctly. Falling back to UTF-16BE for anything
/// outside the safe range costs a few extra bytes; it never costs
/// correctness.
fn encode_pdf_text_string(text: &str) -> Vec<u8> {
    if text.chars().all(|c| c.is_ascii() && !c.is_ascii_control()) {
        return text.as_bytes().to_vec();
    }

    let mut bytes = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{EditLog, PdfDateOffset};

    fn sample_info() -> DocumentInfo {
        DocumentInfo {
            title: Some("Contrato".to_string()),
            author: Some("Ada Lovelace".to_string()),
            subject: None,
            keywords: None,
            creator: Some("pdf-editor-mvp".to_string()),
            producer: None,
            creation_date: Some(PdfDate {
                year: 2026,
                month: 8,
                day: 31,
                hour: 12,
                minute: 0,
                second: 0,
                offset: PdfDateOffset::Utc,
            }),
            mod_date: None,
        }
    }

    fn blank_lopdf_document() -> lopdf::Document {
        lopdf::Document::with_version("1.7")
    }

    fn info_dict(doc: &lopdf::Document) -> &Dictionary {
        let info_id = doc.trailer.get(b"Info").unwrap().as_reference().unwrap();
        doc.get_dictionary(info_id).unwrap()
    }

    fn text_field(dict: &Dictionary, key: &[u8]) -> Option<String> {
        let bytes = dict.get(key).ok()?.as_str().ok()?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    #[test]
    fn creates_the_info_dict_when_the_file_has_none() {
        let mut doc = blank_lopdf_document();
        assert!(doc.trailer.get(b"Info").is_err());

        apply_document_info(&mut doc, &sample_info()).unwrap();

        assert_eq!(
            text_field(info_dict(&doc), b"Title"),
            Some("Contrato".to_string())
        );
    }

    #[test]
    fn writes_every_present_text_field() {
        let mut doc = blank_lopdf_document();
        apply_document_info(&mut doc, &sample_info()).unwrap();

        let dict = info_dict(&doc);
        assert_eq!(text_field(dict, b"Title"), Some("Contrato".to_string()));
        assert_eq!(
            text_field(dict, b"Author"),
            Some("Ada Lovelace".to_string())
        );
        assert_eq!(
            text_field(dict, b"Creator"),
            Some("pdf-editor-mvp".to_string())
        );
    }

    /// Decision 3: `None` means the key is absent from `/Info`, never a key
    /// present with an empty string.
    #[test]
    fn absent_fields_are_not_written_at_all() {
        let mut doc = blank_lopdf_document();
        apply_document_info(&mut doc, &sample_info()).unwrap();

        let dict = info_dict(&doc);
        assert!(!dict.has(b"Subject"));
        assert!(!dict.has(b"Keywords"));
        assert!(!dict.has(b"Producer"));
        assert!(!dict.has(b"ModDate"));
    }

    /// A second apply with a field newly cleared must remove the key a
    /// previous apply wrote — "vaciar un campo... borra la clave al
    /// guardar" (acceptance criteria), exercised here directly against
    /// re-application rather than through the UI that will trigger it.
    #[test]
    fn clearing_a_field_on_a_later_apply_removes_its_key() {
        let mut doc = blank_lopdf_document();
        apply_document_info(&mut doc, &sample_info()).unwrap();
        assert!(info_dict(&doc).has(b"Title"));

        let cleared = DocumentInfo {
            title: None,
            ..sample_info()
        };
        apply_document_info(&mut doc, &cleared).unwrap();

        assert!(!info_dict(&doc).has(b"Title"));
    }

    /// `Some(String::new())` is a distinct state from `None` per decision 3
    /// — the field is present with an empty value, not absent.
    #[test]
    fn an_explicit_empty_string_is_written_not_removed() {
        let mut doc = blank_lopdf_document();
        let info = DocumentInfo {
            title: Some(String::new()),
            ..DocumentInfo::default()
        };

        apply_document_info(&mut doc, &info).unwrap();

        assert!(info_dict(&doc).has(b"Title"));
        assert_eq!(text_field(info_dict(&doc), b"Title"), Some(String::new()));
    }

    #[test]
    fn writes_creation_and_mod_dates_as_pdf_date_strings() {
        let mut doc = blank_lopdf_document();
        let info = DocumentInfo {
            mod_date: Some(PdfDate::parse("D:20260901083000+05'30'").unwrap()),
            ..sample_info()
        };

        apply_document_info(&mut doc, &info).unwrap();

        let dict = info_dict(&doc);
        assert_eq!(
            text_field(dict, b"CreationDate"),
            Some("D:20260831120000Z".to_string())
        );
        assert_eq!(
            text_field(dict, b"ModDate"),
            Some("D:20260901083000+05'30'".to_string())
        );
    }

    /// Printable ASCII stays a literal string — no BOM, no widening — so a
    /// file with only Western/English metadata keeps producing the compact
    /// form every reader expects.
    #[test]
    fn ascii_text_is_written_without_a_byte_order_mark() {
        let mut doc = blank_lopdf_document();
        let info = DocumentInfo {
            title: Some("Report".to_string()),
            ..DocumentInfo::default()
        };

        apply_document_info(&mut doc, &info).unwrap();

        let bytes = info_dict(&doc).get(b"Title").unwrap().as_str().unwrap();
        assert_eq!(bytes, b"Report");
    }

    /// Decision 7: text outside PDFDocEncoding's unambiguous range is never
    /// rejected — it goes out as UTF-16BE with a leading FE FF BOM, which a
    /// spec-compliant reader (and `pdf_manip::document`'s own decoder) must
    /// recognize to read it back correctly.
    #[test]
    fn non_ascii_text_is_written_as_utf16_be_with_a_bom() {
        let mut doc = blank_lopdf_document();
        let info = DocumentInfo {
            title: Some("Título —日本語".to_string()),
            ..DocumentInfo::default()
        };

        apply_document_info(&mut doc, &info).unwrap();

        let bytes = info_dict(&doc).get(b"Title").unwrap().as_str().unwrap();
        assert_eq!(&bytes[..2], &[0xFE, 0xFF], "must open with the BOM");

        let decoded: String = char::decode_utf16(
            bytes[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_be_bytes(*pair)),
        )
        .map(|unit| unit.unwrap())
        .collect();
        assert_eq!(decoded, "Título —日本語");
    }

    #[test]
    fn pending_document_info_finds_the_last_set_document_info_command() {
        let mut document = Document::default();
        let mut log = EditLog::new();
        log.apply(
            &mut document,
            Command::SetDocumentInfo {
                before: DocumentInfo::default(),
                after: DocumentInfo {
                    title: Some("first".to_string()),
                    ..DocumentInfo::default()
                },
            },
        );
        log.apply(
            &mut document,
            Command::SetDocumentInfo {
                before: DocumentInfo {
                    title: Some("first".to_string()),
                    ..DocumentInfo::default()
                },
                after: DocumentInfo {
                    title: Some("second".to_string()),
                    ..DocumentInfo::default()
                },
            },
        );
        document.pending_edits = log;

        assert_eq!(
            pending_document_info(&document).and_then(|info| info.title.clone()),
            Some("second".to_string())
        );
    }

    #[test]
    fn pending_document_info_is_none_after_undo() {
        let mut document = Document::default();
        let mut log = EditLog::new();
        log.apply(
            &mut document,
            Command::SetDocumentInfo {
                before: DocumentInfo::default(),
                after: DocumentInfo {
                    title: Some("first".to_string()),
                    ..DocumentInfo::default()
                },
            },
        );
        log.undo(&mut document);
        document.pending_edits = log;

        assert_eq!(pending_document_info(&document), None);
    }

    #[test]
    fn pending_document_info_is_none_without_any_set_document_info_command() {
        let document = Document::default();

        assert_eq!(pending_document_info(&document), None);
    }
}
