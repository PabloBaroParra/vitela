//! Perf harness (T-021): `spec.md` "Large-File Performance" — "Given a 50MB,
//! 200-page PDF ... calling OpenDocument() MUST fire the PageRendered event
//! for page 1 in under 1.5s, and the thumbnail sidebar MUST populate within
//! 3s total."
//!
//! `#[ignore]`d: generating/opening a ~50MB fixture is slow and disk-heavy,
//! and wall-clock perf assertions on shared/CI hardware are inherently
//! noisier than functional tests. Run explicitly:
//!
//! ```sh
//! cargo test -p pdf-render --test perf_large_fixture -- --ignored --nocapture
//! ```
//!
//! Real numbers from a manual run on this machine are recorded in this
//! batch's apply-progress record (`sdd/pdf-editor-mvp/b3-apply-progress`) —
//! per the "no faked numbers" instruction, this file only asserts; it does
//! not claim what the numbers will be ahead of actually running it.

use std::time::Instant;

use gen_fixtures::large::{generate_large_fixture, PERF_LARGE_SPEC};
use pdf_render::{PdfiumRenderer, Priority, RenderOptions};

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/large/perf_200pg.pdf")
}

fn ensure_fixture() -> std::path::PathBuf {
    let path = fixture_path();
    if !path.exists() {
        generate_large_fixture(&path, &PERF_LARGE_SPEC).expect("generate perf fixture");
    }
    path
}

#[test]
#[ignore = "perf harness: generates/opens a ~50MB fixture, wall-clock assertions; run explicitly, see module docs"]
fn page_one_renders_under_1_5s_and_thumbnails_populate_under_3s() {
    let path = ensure_fixture();
    let renderer = PdfiumRenderer::new();

    let open_start = Instant::now();
    let doc = renderer
        .open_document(&path, None)
        .expect("open perf fixture");
    let page_one_start = Instant::now();
    let page_one = renderer
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
    let page_one_elapsed = page_one_start.elapsed();
    let open_to_page_one_elapsed = open_start.elapsed();

    assert!(page_one.width().unwrap() > 0);
    println!(
        "OpenDocument()->page 1 rendered: {:?} (render_page call alone: {:?})",
        open_to_page_one_elapsed, page_one_elapsed
    );
    assert!(
        open_to_page_one_elapsed.as_secs_f64() < 1.5,
        "page 1 must render in <1.5s from OpenDocument(), took {:?}",
        open_to_page_one_elapsed
    );

    let thumbnails_start = Instant::now();
    let thumbnail_handles: Vec<_> = (0..PERF_LARGE_SPEC.pages)
        .map(|page_index| {
            renderer.render_page(
                doc,
                page_index,
                36, // thumbnail-strip DPI: small, representative of a sidebar preview
                None,
                RenderOptions::default(),
                Priority::Thumbnail,
            )
        })
        .collect();
    for handle in thumbnail_handles {
        handle.wait().expect("each thumbnail must render");
    }
    let thumbnails_elapsed = thumbnails_start.elapsed();

    println!(
        "Thumbnail sidebar ({} pages) populated in: {:?}",
        PERF_LARGE_SPEC.pages, thumbnails_elapsed
    );
    assert!(
        thumbnails_elapsed.as_secs_f64() < 3.0,
        "thumbnail sidebar must populate in <3s total, took {:?}",
        thumbnails_elapsed
    );
}
