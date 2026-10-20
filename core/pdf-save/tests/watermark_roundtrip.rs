//! Integration tests for the "Add watermark" tool: open a real file, record
//! a watermark, save through the public API, reopen, and read the stamp back
//! out of every page it named — including after the pages it named have
//! moved or been deleted in the same batch.

use pdf_document::{Command, Document, PageId, Watermark, WatermarkLayer};
use pdf_save::{save_document, save_preview, SaveInput, SaveIntent, SignatureAcknowledgement};

/// Bumped on every call: parallel test threads can read the same
/// `SystemTime` on Windows, so pid + nanos alone is not unique.
static TEMP_DIR_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn temp_pdf_path(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pdf-save-watermark-{label}-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        TEMP_DIR_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("doc.pdf")
}

/// A three-page file whose pages read "doc page 0", "doc page 1", ...
fn three_pages() -> (Document, pdf_manip::LopdfDocument, Vec<u8>) {
    let mut fixture = gen_fixtures::build_multi_page_document(3, "doc");
    let path = temp_pdf_path("three-pages");
    fixture.save(&path).unwrap();
    let original_bytes = std::fs::read(&path).unwrap();
    let (base, security) = pdf_manip::open_document(&path, None).unwrap();
    let document = pdf_save::document_from_lopdf(&base, security).unwrap();
    (document, base, original_bytes)
}

fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    assert!(log.apply(document, command), "recordable");
    document.pending_edits = log;
}

fn input<'a>(
    document: &'a Document,
    base: &'a pdf_manip::LopdfDocument,
    original_bytes: &'a [u8],
) -> SaveInput<'a> {
    SaveInput {
        document,
        base,
        original_bytes: Some(original_bytes),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
        imported_sources: pdf_save::ImportedSources::none(),
    }
}

/// Every page of `bytes`, as the list of texts it paints.
fn pages(bytes: &[u8]) -> Vec<Vec<String>> {
    let reloaded = lopdf::Document::load_mem(bytes).expect("output must reload");
    (0..reloaded.get_pages().len() as u32)
        .map(|index| {
            pdf_edit::read_page_content(&reloaded, PageId(index))
                .expect("readable page")
                .text_runs
                .into_iter()
                .map(|run| run.text)
                .collect()
        })
        .collect()
}

fn texts(page: &[&str]) -> Vec<String> {
    page.iter().map(|text| text.to_string()).collect()
}

#[test]
fn a_watermark_reaches_every_page_it_names_and_no_other() {
    let (mut document, base, original_bytes) = three_pages();
    apply_command(
        &mut document,
        Command::AddWatermark(Watermark::diagonal(vec![PageId(0), PageId(2)], "DRAFT")),
    );

    let saved = save_document(input(&document, &base, &original_bytes)).expect("saves");

    assert_eq!(
        pages(&saved),
        vec![
            texts(&["doc page 0", "DRAFT"]),
            texts(&["doc page 1"]),
            texts(&["doc page 2", "DRAFT"]),
        ]
    );
}

/// The stamp names pages by identity, so it follows them through a move
/// recorded after it and skips one deleted after it.
#[test]
fn a_watermark_follows_its_pages_through_later_moves_and_deletions() {
    let (mut document, base, original_bytes) = three_pages();
    apply_command(
        &mut document,
        Command::AddWatermark(Watermark::diagonal(vec![PageId(1), PageId(2)], "COPY")),
    );
    apply_command(&mut document, Command::MovePage { from: 2, to: 0 });
    let removal = Command::remove_page(&document, 2).expect("page at index 2");
    apply_command(&mut document, removal); // removes PageId(1)

    let saved = save_document(input(&document, &base, &original_bytes)).expect("saves");

    assert_eq!(
        pages(&saved),
        vec![texts(&["doc page 2", "COPY"]), texts(&["doc page 0"])]
    );
}

/// What a shell renders before the user saves: the preview carries the
/// stamp, so the watermark is visible the moment it is recorded.
#[test]
fn the_preview_shows_the_watermark_before_it_is_saved() {
    let (mut document, base, original_bytes) = three_pages();
    apply_command(
        &mut document,
        Command::AddWatermark(Watermark::diagonal(vec![PageId(1)], "DRAFT")),
    );

    let preview = save_preview(input(&document, &base, &original_bytes)).expect("previews");

    assert_eq!(pages(&preview)[1], texts(&["doc page 1", "DRAFT"]));
}

/// Every page of `bytes`, as the number of images it paints.
fn image_counts(bytes: &[u8]) -> Vec<usize> {
    let reloaded = lopdf::Document::load_mem(bytes).expect("output must reload");
    (0..reloaded.get_pages().len() as u32)
        .map(|index| {
            pdf_edit::read_page_content(&reloaded, PageId(index))
                .expect("readable page")
                .images
                .len()
        })
        .collect()
}

/// A picture behind the content survives the save on every page it names,
/// leaves the page's own text readable, and is stored once.
#[test]
fn a_picture_background_reaches_every_page_it_names_as_one_image() {
    let (mut document, base, original_bytes) = three_pages();
    let picture = Watermark::picture(
        vec![PageId(0), PageId(2)],
        gen_fixtures::content_edit::replacement_image_png_bytes(),
    );
    assert_eq!(picture.layer, WatermarkLayer::UnderContent);
    apply_command(&mut document, Command::AddWatermark(picture));

    let saved = save_document(input(&document, &base, &original_bytes)).expect("saves");

    assert_eq!(image_counts(&saved), vec![1, 0, 1]);
    assert_eq!(
        pages(&saved),
        vec![
            texts(&["doc page 0"]),
            texts(&["doc page 1"]),
            texts(&["doc page 2"]),
        ]
    );
    let reloaded = lopdf::Document::load_mem(&saved).unwrap();
    let images = reloaded
        .objects
        .values()
        .filter(|object| {
            object.as_stream().is_ok_and(|stream| {
                stream
                    .dict
                    .get(b"Subtype")
                    .and_then(lopdf::Object::as_name)
                    .ok()
                    == Some(b"Image")
            })
        })
        .count();
    assert_eq!(images, 1, "one image object shared by both pages");
}

/// A text stamp sent to the background reads before the page's own words.
#[test]
fn a_text_background_is_painted_before_the_page_text() {
    let (mut document, base, original_bytes) = three_pages();
    apply_command(
        &mut document,
        Command::AddWatermark(Watermark {
            layer: WatermarkLayer::UnderContent,
            ..Watermark::diagonal(vec![PageId(1)], "DRAFT")
        }),
    );

    let saved = save_document(input(&document, &base, &original_bytes)).expect("saves");

    assert_eq!(pages(&saved)[1], texts(&["DRAFT", "doc page 1"]));
}
