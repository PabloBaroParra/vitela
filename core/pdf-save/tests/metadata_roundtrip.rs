//! External-interop fixtures for Batch 22's metadata save path (T-174): a
//! PDF with a fully populated `/Info` (seven fields + valid dates), one with
//! none at all, and a non-Latin1 `/Title` — pinning the UTF-16BE+BOM path
//! (batch decision 7). Unlike `metadata.rs`'s and `strategy.rs`'s unit
//! tests, which only prove `pdf-save`'s own reader agrees with its own
//! writer, these two `#[ignore]`d tests write a caller-owned output file for
//! `tools/pypdf-validation/validate_metadata_roundtrip.py` to check with an
//! independent PDF library — the same split `content_edit_roundtrip.rs`'s
//! `write_pypdf_validation_output` (T-160) already uses.

use pdf_document::{Command, Document, DocumentInfo, PdfDate};
use pdf_save::{
    document_from_lopdf, save_document, SaveInput, SaveIntent, SignatureAcknowledgement,
};

fn temp_pdf_path(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pdf-save-metadata-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("doc.pdf")
}

fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}

fn save_with_original(
    document: &Document,
    base: &pdf_manip::LopdfDocument,
    original_bytes: &[u8],
) -> Vec<u8> {
    save_document(SaveInput {
        document,
        base,
        original_bytes: Some(original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
    })
    .expect("save should succeed")
}

/// A real on-disk one-page PDF with all seven `/Info` text keys plus a valid
/// `/CreationDate` already set — the "fixture with a complete `/Info`" T-174
/// asks for.
fn one_page_pdf_with_populated_info() -> std::path::PathBuf {
    use lopdf::{dictionary, Object};

    let mut doc = lopdf::Document::with_version("1.5");
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });
    let pages_id = doc.add_object(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1,
    });
    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(page_id) {
        dict.set("Parent", pages_id);
    }
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut info = lopdf::Dictionary::new();
    info.set("Title", Object::string_literal("Informe Original"));
    info.set("Author", Object::string_literal("Ada Lovelace"));
    info.set("Subject", Object::string_literal("Reporte trimestral"));
    info.set("Keywords", Object::string_literal("finanzas, Q3"));
    info.set("Creator", Object::string_literal("pdf-editor-mvp"));
    info.set("Producer", Object::string_literal("pdf-editor-mvp"));
    info.set("CreationDate", Object::string_literal("D:20250115093000Z"));
    let info_id = doc.add_object(Object::Dictionary(info));
    doc.trailer.set("Info", info_id);

    let path = temp_pdf_path("populated-info");
    doc.save(&path).unwrap();
    path
}

/// A real on-disk one-page PDF with no `/Info` entry at all — the "fixture
/// with none" T-174 asks for.
fn one_page_pdf_without_info() -> std::path::PathBuf {
    use lopdf::{dictionary, Object};

    let mut doc = lopdf::Document::with_version("1.5");
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });
    let pages_id = doc.add_object(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1,
    });
    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(page_id) {
        dict.set("Parent", pages_id);
    }
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let path = temp_pdf_path("no-info");
    doc.save(&path).unwrap();
    path
}

/// Retypes only `/Title`, to a non-Latin1 string, on a document that already
/// carries a full `/Info` — every other field rides through the "before"
/// snapshot unchanged, so `validate_metadata_roundtrip.py`'s "populated"
/// check also proves the untouched fields (and the original, unedited
/// `/CreationDate`) survive a metadata-only save.
#[test]
#[ignore = "writes a caller-owned file for the standalone pypdf validator"]
fn write_pypdf_validation_output_for_populated_info() {
    let output = std::env::var_os("PDF_METADATA_VALIDATION_OUTPUT_POPULATED")
        .map(std::path::PathBuf::from)
        .expect("PDF_METADATA_VALIDATION_OUTPUT_POPULATED must name a caller-owned output file");
    assert!(
        !output.exists(),
        "refusing to overwrite caller-owned output"
    );
    std::fs::create_dir_all(output.parent().expect("output must have a parent"))
        .expect("create caller-owned output parent");

    let path = one_page_pdf_with_populated_info();
    let original_bytes = std::fs::read(&path).expect("read fixture");
    let (base, security) = pdf_manip::open_document(&path, None).expect("open fixture");
    let mut document = document_from_lopdf(&base, security).expect("convert fixture");

    let before = base.document_info();
    apply_command(
        &mut document,
        Command::SetDocumentInfo {
            before: before.clone(),
            after: DocumentInfo {
                // Decision 7: outside PDFDocEncoding's unambiguous printable-ASCII
                // range, so this must round-trip through UTF-16BE with a BOM.
                title: Some("Título — 日本語 café".to_string()),
                ..before
            },
        },
    );

    std::fs::write(
        &output,
        save_with_original(&document, &base, &original_bytes),
    )
    .expect("write only the caller-owned output");
}

/// Writes a brand-new `/Info` dict, all seven fields plus a valid
/// `/CreationDate`, onto a document that started with none.
#[test]
#[ignore = "writes a caller-owned file for the standalone pypdf validator"]
fn write_pypdf_validation_output_for_created_info() {
    let output = std::env::var_os("PDF_METADATA_VALIDATION_OUTPUT_CREATED")
        .map(std::path::PathBuf::from)
        .expect("PDF_METADATA_VALIDATION_OUTPUT_CREATED must name a caller-owned output file");
    assert!(
        !output.exists(),
        "refusing to overwrite caller-owned output"
    );
    std::fs::create_dir_all(output.parent().expect("output must have a parent"))
        .expect("create caller-owned output parent");

    let path = one_page_pdf_without_info();
    let original_bytes = std::fs::read(&path).expect("read fixture");
    let (base, security) = pdf_manip::open_document(&path, None).expect("open fixture");
    assert!(base.as_lopdf().trailer.get(b"Info").is_err());
    let mut document = document_from_lopdf(&base, security).expect("convert fixture");

    apply_command(
        &mut document,
        Command::SetDocumentInfo {
            before: DocumentInfo::default(),
            after: DocumentInfo {
                title: Some("Contrato Digital".to_string()),
                author: Some("Equipo Legal".to_string()),
                subject: Some("Términos y condiciones".to_string()),
                keywords: Some("contrato, legal, digital".to_string()),
                creator: Some("pdf-editor-mvp".to_string()),
                producer: Some("pdf-editor-mvp".to_string()),
                creation_date: Some(PdfDate::parse("D:20260115093000Z").unwrap()),
                mod_date: None,
            },
        },
    );

    std::fs::write(
        &output,
        save_with_original(&document, &base, &original_bytes),
    )
    .expect("write only the caller-owned output");
}
