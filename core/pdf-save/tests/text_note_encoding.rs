//! A note typed with non-ASCII characters must reach the file as a real PDF
//! text string (ISO 32000-2 §7.9.2.2): UTF-16BE with a `FE FF` BOM, never
//! raw UTF-8, or Acrobat and every other viewer shows mojibake.

use pdf_document::{Annotation, AnnotationId, AnnotationKind, Command, PageId, Popup, Rect};
use pdf_save::{save_document, SaveInput, SaveIntent, SignatureAcknowledgement};

fn one_page_pdf(label: &str) -> std::path::PathBuf {
    use lopdf::{dictionary, Object};

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);

    let dir = std::env::temp_dir().join(format!(
        "pdf-save-note-encoding-{label}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("doc.pdf");
    doc.save(&path).unwrap();
    path
}

/// Saves a document carrying one text note and returns the reopened markup
/// and popup `/Contents` raw bytes.
fn saved_note_contents(label: &str, text: &str) -> (Vec<u8>, Vec<u8>) {
    let path = one_page_pdf(label);
    let original = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let mut document = pdf_save::document_from_lopdf(&base, security).unwrap();
    let page = document.pages[0].id;

    let note = Annotation {
        id: AnnotationId(1),
        page: PageId(page.0),
        kind: AnnotationKind::TextNote {
            rect: Rect {
                x: 10.0,
                y: 10.0,
                width: 20.0,
                height: 20.0,
            },
            contents: text.to_string(),
            popup: Popup {
                is_open: false,
                contents: text.to_string(),
            },
        },
    };
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(&mut document, Command::AddAnnotation(note));
    document.pending_edits = log;

    let saved = save_document(SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
        imported_sources: pdf_save::ImportedSources::none(),
    })
    .expect("save");

    let reloaded = lopdf::Document::load_mem(&saved).expect("reload");
    let page_id = *reloaded.get_pages().get(&1).unwrap();
    let annots = reloaded
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Annots")
        .and_then(|o| o.as_array())
        .unwrap()
        .clone();
    let markup = reloaded
        .get_dictionary(annots[0].as_reference().unwrap())
        .unwrap();
    let popup = reloaded
        .get_dictionary(markup.get(b"Popup").unwrap().as_reference().unwrap())
        .unwrap();
    (
        markup.get(b"Contents").unwrap().as_str().unwrap().to_vec(),
        popup.get(b"Contents").unwrap().as_str().unwrap().to_vec(),
    )
}

#[test]
fn non_ascii_note_round_trips_as_utf16_be() {
    let (markup, popup) = saved_note_contents("utf16", "Ñandú €");
    for bytes in [&markup, &popup] {
        assert_eq!(&bytes[..2], &[0xFE, 0xFF], "must open with the BOM");
        let object = lopdf::Object::string_literal(bytes.clone());
        assert_eq!(lopdf::decode_text_string(&object).unwrap(), "Ñandú €");
    }
}

#[test]
fn ascii_note_is_still_written_as_plain_bytes() {
    let (markup, popup) = saved_note_contents("ascii", "hello note");
    assert_eq!(markup, b"hello note");
    assert_eq!(popup, b"hello note");
}
