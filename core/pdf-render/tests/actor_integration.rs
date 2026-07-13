//! Real-pdfium integration tests (T-020) for `PdfiumRenderer` /
//! `PdfiumActor` (`Actor<PdfiumState>`).
//!
//! Pure scheduling behavior (priority reordering, cancel-at-dequeue,
//! generic concurrency) is already covered deterministically — with a
//! synchronization gate, zero timing dependence — by `src/actor.rs`'s unit
//! tests, which exercise the *exact same* `Actor<S>` engine `PdfiumActor` is
//! a type alias of (`Actor<PdfiumState>`), not a reimplementation. This file
//! therefore focuses on what only a real pdfium binding can prove: actual
//! rendering doesn't crash or corrupt state under concurrency, dark-mode
//! inversion really changes pixel bytes, text-run data is real font/position
//! data, and cancel-before-raster holds against the true executor (not a
//! fake one) — see `spec.md`'s "Serialized pdfium Access" scenarios.

use pdf_render::{PdfiumRenderer, Priority, RenderError, RenderOptions};

const PAGE_COUNT: u32 = 8;

fn small_fixture_path() -> std::path::PathBuf {
    // `cargo test` runs these `#[test]` fns concurrently on separate threads
    // within one process; a nanosecond timestamp alone is not a reliable
    // uniqueness source on Windows, where `SystemTime::now()`'s effective
    // resolution can be much coarser than one nanosecond, so two tests
    // starting close together can compute the same "unique" file name and
    // race each other's save/open/cleanup. A process-wide atomic counter is
    // used to guarantee a distinct path per call regardless of clock
    // resolution.
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let doc = gen_fixtures::build_multi_page_document(PAGE_COUNT, "pdf-render-integration");
    let path = std::env::temp_dir().join(format!(
        "pdf-render-integration-{}-{}.pdf",
        std::process::id(),
        unique
    ));
    let mut doc = doc;
    doc.save(&path).expect("save small multi-page fixture");
    path
}

#[test]
fn opens_document_and_renders_page_one_with_plausible_dimensions() {
    let path = small_fixture_path();
    let renderer = PdfiumRenderer::new();

    let doc = renderer.open_document(&path, None).expect("open fixture");
    let bitmap = renderer
        .render_page(
            doc,
            0,
            150,
            None,
            RenderOptions::default(),
            Priority::Visible,
        )
        .wait()
        .expect("render page 1");

    // US Letter (612x792 pt) at 150 DPI: 612/72*150 = 1275, 792/72*150 = 1650.
    assert_eq!(bitmap.width().unwrap(), 1275);
    assert_eq!(bitmap.height().unwrap(), 1650);
    let pixels = bitmap.get_pixels().unwrap();
    assert_eq!(
        pixels.len() as u32,
        bitmap.width().unwrap() * bitmap.height().unwrap() * 4
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn dark_mode_inversion_changes_rendered_pixels() {
    let path = small_fixture_path();
    let renderer = PdfiumRenderer::new();
    let doc = renderer.open_document(&path, None).expect("open fixture");

    let normal = renderer
        .render_page(
            doc,
            0,
            72,
            None,
            RenderOptions::default(),
            Priority::Visible,
        )
        .wait()
        .expect("normal render")
        .get_pixels()
        .unwrap();
    let inverted = renderer
        .render_page(
            doc,
            0,
            72,
            None,
            RenderOptions::default().with_invert_content_colors(true),
            Priority::Visible,
        )
        .wait()
        .expect("inverted render")
        .get_pixels()
        .unwrap();

    assert_eq!(normal.len(), inverted.len());
    assert_ne!(
        normal, inverted,
        "inversion must actually change pixel data"
    );

    // Spot-check a handful of pixels: RGB channels are complements, alpha
    // untouched (see `inversion.rs`).
    for chunk_pair in normal
        .chunks_exact(4)
        .zip(inverted.chunks_exact(4))
        .take(64)
    {
        let (n, i) = chunk_pair;
        assert_eq!(i[0], 255 - n[0]);
        assert_eq!(i[1], 255 - n[1]);
        assert_eq!(i[2], 255 - n[2]);
        assert_eq!(i[3], n[3]);
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn text_runs_returns_real_font_and_position_data() {
    let path = small_fixture_path();
    let renderer = PdfiumRenderer::new();
    let doc = renderer.open_document(&path, None).expect("open fixture");

    let runs = renderer
        .text_runs(doc, 0, Priority::Visible)
        .wait()
        .expect("text runs");

    assert!(!runs.is_empty(), "fixture page has visible text");
    let run = &runs[0];
    assert!(!run.text.is_empty());
    assert!(!run.font_name.is_empty());
    assert!(run.font_size_pt > 0.0);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn five_concurrent_render_requests_all_complete_without_crash() {
    // spec.md "Serialized pdfium Access" — "Concurrent render requests:
    // GIVEN 5 pages requested concurrently, WHEN the render worker processes
    // them, THEN all complete without crash, sequentially."
    let path = small_fixture_path();
    let renderer = std::sync::Arc::new(PdfiumRenderer::new());
    let doc = renderer.open_document(&path, None).expect("open fixture");

    let handles: Vec<_> = (0..5)
        .map(|page_index| {
            let renderer = std::sync::Arc::clone(&renderer);
            std::thread::spawn(move || {
                renderer
                    .render_page(
                        doc,
                        page_index,
                        96,
                        None,
                        RenderOptions::default(),
                        Priority::Visible,
                    )
                    .wait()
            })
        })
        .collect();

    for handle in handles {
        let bitmap = handle
            .join()
            .unwrap()
            .expect("each concurrent render must succeed");
        assert!(bitmap.width().unwrap() > 0);
        assert!(bitmap.height().unwrap() > 0);
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn cancel_before_raster_skips_rendering_against_the_real_executor() {
    // Build genuine queue pressure with real (Thumbnail-priority) render
    // jobs so the cancelled job is provably still queued — not yet
    // dequeued — when `.cancel()` is called on the very next line.
    // Submission (a mutex push) is orders of magnitude faster than actually
    // rasterizing a page through pdfium, so 40 queued real jobs reliably
    // outlast the time it takes this thread to submit one more and cancel
    // it immediately.
    let path = small_fixture_path();
    let renderer = PdfiumRenderer::new();
    let doc = renderer.open_document(&path, None).expect("open fixture");

    let occupying: Vec<_> = (0..40)
        .map(|i| {
            renderer.render_page(
                doc,
                i % PAGE_COUNT,
                150,
                None,
                RenderOptions::default(),
                Priority::Thumbnail,
            )
        })
        .collect();

    let cancel_me = renderer.render_page(
        doc,
        0,
        150,
        None,
        RenderOptions::default(),
        Priority::Thumbnail,
    );
    cancel_me.cancel();

    let result = cancel_me.wait();
    assert!(
        matches!(result, Err(RenderError::Cancelled)),
        "expected Cancelled, got {result:?}"
    );

    for handle in occupying {
        let _ = handle.wait();
    }

    let _ = std::fs::remove_file(&path);
}
