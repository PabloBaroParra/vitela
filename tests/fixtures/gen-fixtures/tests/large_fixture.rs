//! Perf-harness fixture generation (Batch 3, T-021): produces the
//! ~50MB/200-page PDF consumed by `pdf-render`'s perf test
//! (`core/pdf-render/tests/perf_large_fixture.rs`).
//!
//! `#[ignore]`d like the perf test itself: generating ~50MB is slow and
//! unnecessary on every `cargo test` run. Run explicitly with
//! `cargo test -p gen-fixtures --test large_fixture -- --ignored`.

use gen_fixtures::large::{generate_large_fixture, PERF_LARGE_SPEC};

#[test]
#[ignore = "generates a ~50MB fixture; run explicitly, see module docs"]
fn generates_fixture_close_to_the_spec_reference_size() {
    let out_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../large/perf_200pg.pdf");

    generate_large_fixture(&out_path, &PERF_LARGE_SPEC).expect("fixture generation must succeed");

    let metadata = std::fs::metadata(&out_path).unwrap();
    let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
    println!(
        "generated {} pages, image_side_px={} -> {:.2} MB at {}",
        PERF_LARGE_SPEC.pages,
        PERF_LARGE_SPEC.image_side_px,
        size_mb,
        out_path.display()
    );

    // spec.md describes "a 50MB, 200-page PDF" as the reference-hardware
    // fixture; `image_side_px` is tuned so the real generated size lands in
    // this range rather than asserting an exact byte count (DEFLATE ratio on
    // the gradient+noise pattern isn't perfectly predictable analytically).
    assert!(
        (35.0..=65.0).contains(&size_mb),
        "expected fixture size roughly near 50MB, got {size_mb:.2} MB"
    );
}
