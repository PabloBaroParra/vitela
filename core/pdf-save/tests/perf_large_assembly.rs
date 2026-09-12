//! Perf harness for `docs/batch-pdf-assembly.md` §11, item 12: "medir
//! importación, primer render y cambio de modo con documentos grandes".
//!
//! The GTK tests already answer that question in *renders* — a view switch
//! costs zero of them once the thumbnail cache is warm
//! (`organize::tests::thumbnails`). What a render count cannot say is how
//! long the renders it does count actually take on a document big enough to
//! matter, and that is the whole of what §11 still had open. This harness
//! measures the three operations on a real 200-page, ~50MB PDF imported into
//! another copy of itself — a 400-page assembly:
//!
//! - `import` — the legs of `organize::import::prepare` and its `apply`:
//!   opening the source under its copy permission, the graft report, the
//!   page list, the `ImportPages` command, and the save that materializes
//!   the grafted pages.
//! - `first render` — reopening the saved bytes and rendering the first
//!   card, then the rest of the Pages grid, at the DPI the grid asks for.
//! - `mode switch` — rendering one block cover per imported document at the
//!   Documents view's smaller DPI: the cost of entering that view on a cold
//!   cache, which is the worst case the cache exists to avoid paying twice.
//!
//! Like the two harnesses it sits beside, this one does **not** assert a
//! millisecond budget. `spec.md` states one for opening a document (page 1
//! under 1.5s, thumbnail strip under 3s) and none for assembling one, and
//! inventing a number here would fix the answer before measuring it. The
//! assertions cover only what must hold for the timings to mean anything —
//! that the assembly really is 400 pages and that every render came back
//! with pixels. The numbers come out on `--nocapture` and are recorded in
//! the checklist's §11 progress notes.
//!
//! `#[ignore]`d for the same reason as `pdf-render`'s `perf_large_fixture`:
//! it generates and opens a ~50MB fixture and measures wall clock. Run it
//! explicitly:
//!
//! ```sh
//! cargo test --release -p pdf-save --test perf_large_assembly -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gen_fixtures::large::{generate_large_fixture, PERF_LARGE_SPEC};
use pdf_document::{Command, EditLog, ImportedDocumentId};
use pdf_manip::LopdfDocument;
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderOptions};
use pdf_save::{save_document, ImportedSources, SaveInput, SaveIntent, SignatureAcknowledgement};

/// The DPI the Organize grid asks pdfium for on this fixture's US Letter
/// pages. `organize::grid::thumbnail::thumbnail_dpi` fits the page into a
/// 140x180 px card at the 3x render headroom, which on 612x792 pt floors to
/// 49; the Documents view fits the same page into a 96x124 px block cover,
/// which floors to 33.
///
/// Copied rather than shared: `linux-gtk` is `cfg`-gated to Linux and cannot
/// be a dependency of a core test, and a shell must not depend on one either.
/// If the card sizes move, these numbers are a measurement input to update,
/// not behavior to keep in sync.
const GRID_CARD_DPI: u32 = 49;
const BLOCK_COVER_DPI: u32 = 33;

/// How many blocks the Documents view draws for the assembly below: the base
/// document and the one PDF imported into it.
const BLOCKS: u32 = 2;

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/large/perf_200pg.pdf")
}

/// The same ~50MB fixture `pdf-render`'s harness uses, generated on first run
/// and reused afterwards — it is deliberately not committed.
fn ensure_fixture() -> PathBuf {
    let path = fixture_path();
    if !path.exists() {
        generate_large_fixture(&path, &PERF_LARGE_SPEC).expect("generate perf fixture");
    }
    path
}

/// One timed leg and what it produced, so the caller keeps the value without
/// having to thread an `Instant` through every step.
fn timed<T>(work: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let value = work();
    (value, start.elapsed())
}

fn render_page_at(renderer: &PdfiumRenderer, handle: DocumentHandle, index: u32, dpi: u32) {
    let page = renderer
        .render_page(
            handle,
            index,
            dpi,
            None,
            RenderOptions::default(),
            Priority::Thumbnail,
        )
        .wait()
        .expect("render a card");
    assert!(
        page.width().expect("a rendered card exposes a width") > 0,
        "an empty bitmap would make every timing here meaningless"
    );
}

fn millis(label: &str, elapsed: Duration) {
    println!("  {label:<24} {:>9.1} ms", elapsed.as_secs_f64() * 1000.0);
}

#[test]
#[ignore = "perf harness: generates/opens a ~50MB fixture and measures wall clock; run explicitly, see module docs"]
fn importing_rendering_and_switching_views_on_a_large_assembly() {
    let path = ensure_fixture();
    let original_bytes = std::fs::read(&path).expect("read fixture");

    // What the Organize screen needs to exist at all: the lopdf tree the
    // writer rebases on, and the model the page commands edit.
    let ((base, security), open_destination) =
        timed(|| pdf_manip::open_document(&path, None).expect("open destination"));
    let (mut document, model_load) =
        timed(|| pdf_save::document_from_lopdf(&base, security).expect("model from destination"));

    // `prepare`'s legs, in the order the shell runs them.
    let ((source, _), open_source) = timed(|| {
        pdf_manip::open_import_source_from_bytes(&original_bytes, None).expect("open source")
    });
    let selected: Vec<usize> = (0..source.page_count()).collect();
    let (_, graft_report) =
        timed(|| pdf_manip::graft_report(&source, &selected).expect("graft report"));
    let next_page_id = u32::try_from(document.pages.len()).expect("page count fits in u32");
    let (imported, page_list) = timed(|| {
        pdf_save::imported_pages_from_lopdf(&source, ImportedDocumentId(1), next_page_id)
            .expect("imported page list")
    });
    let imported_count = imported.len();

    // `apply`'s leg: one undoable command for the whole batch.
    let mut log = EditLog::new();
    let index = document.pages.len();
    let (applied, apply) = timed(|| {
        log.apply(
            &mut document,
            Command::ImportPages {
                index,
                pages: imported,
            },
        )
    });
    assert!(applied, "the import should apply to this model");

    let registry = [(ImportedDocumentId(1), &source)];
    let (saved, save) = timed(|| {
        save_document(SaveInput {
            document: &document,
            base: &base,
            original_bytes: Some(&original_bytes),
            intent: SaveIntent::Default,
            signatures: SignatureAcknowledgement::Unacknowledged,
            imported_sources: ImportedSources::new(&registry),
        })
        .expect("save the assembly")
    });

    let pages = u32::try_from(document.pages.len()).expect("page count fits in u32");
    assert_eq!(
        pages as usize,
        PERF_LARGE_SPEC.pages as usize + imported_count,
        "the assembly must hold both documents"
    );
    assert_page_count(&saved, pages);

    // First render: what the user waits for after the import lands, which is
    // a fresh pdfium handle over the saved bytes plus one render per card.
    let renderer = PdfiumRenderer::new();
    let (handle, reopen) = timed(|| {
        renderer
            .open_document_from_bytes(saved.clone(), None)
            .expect("reopen the assembly")
    });
    let (_, first_card) = timed(|| render_page_at(&renderer, handle, 0, GRID_CARD_DPI));
    let (_, grid_fill) = timed(|| {
        for page in 1..pages {
            render_page_at(&renderer, handle, page, GRID_CARD_DPI);
        }
    });

    // Mode switch on a cold cache: the Documents view draws one cover per
    // block, not one per page. A warm cache costs zero renders — that is
    // what `organize::tests::thumbnails` pins.
    let (_, block_covers) = timed(|| {
        for block in 0..BLOCKS {
            render_page_at(
                &renderer,
                handle,
                block * PERF_LARGE_SPEC.pages,
                BLOCK_COVER_DPI,
            );
        }
    });

    println!(
        "\n{} pages ({} base + {imported_count} imported), {:.1} MiB written\n",
        pages,
        PERF_LARGE_SPEC.pages,
        saved.len() as f64 / (1024.0 * 1024.0)
    );
    println!("import");
    millis("open destination", open_destination);
    millis("build model", model_load);
    millis("open source", open_source);
    millis("graft report", graft_report);
    millis("imported page list", page_list);
    millis("ImportPages", apply);
    millis("save", save);
    millis(
        "total",
        open_destination + model_load + open_source + graft_report + page_list + apply + save,
    );
    println!("first render (Pages view, {GRID_CARD_DPI} dpi)");
    millis("reopen saved bytes", reopen);
    millis("first card", first_card);
    millis("remaining cards", grid_fill);
    millis("total", reopen + first_card + grid_fill);
    println!("mode switch (Documents view, {BLOCK_COVER_DPI} dpi, cold cache)");
    millis("block covers", block_covers);
}

fn assert_page_count(bytes: &[u8], expected: u32) {
    let reopened = LopdfDocument::from_lopdf(
        lopdf::Document::load_mem(bytes).expect("the saved bytes reload"),
    );
    assert_eq!(
        reopened.page_count(),
        expected as usize,
        "the saved file must carry every page the model held"
    );
}
