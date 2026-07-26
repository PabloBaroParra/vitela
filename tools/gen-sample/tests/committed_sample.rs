//! Guards the **committed** `assets/sample/vitela-sample.pdf` — the file the
//! three platform shells actually package.
//!
//! The unit tests in `src/lib.rs` prove the generator is correct; these prove
//! the artefact on disk still matches it and is genuinely openable through
//! the production `pdf-manip` path. Without this, a hand-edited or
//! stale-committed sample would ship to every platform unnoticed.

use std::path::PathBuf;

fn committed_sample_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/sample")
        .join(gen_sample::SAMPLE_FILE_NAME)
}

#[test]
fn committed_sample_matches_the_generator() {
    let on_disk = std::fs::read(committed_sample_path())
        .expect("assets/sample/vitela-sample.pdf is committed; run `cargo run -p gen-sample`");
    assert_eq!(
        on_disk,
        gen_sample::sample_bytes().unwrap(),
        "the committed sample is stale — regenerate with `cargo run -p gen-sample`"
    );
}

#[test]
fn committed_sample_opens_unencrypted_with_the_expected_pages() {
    let bytes = std::fs::read(committed_sample_path()).unwrap();
    let (document, security) = pdf_manip::open_document_from_bytes(&bytes, None)
        .expect("the shipped sample must open without a password");

    assert!(
        security.is_none(),
        "the shipped sample must not be encrypted"
    );
    assert_eq!(document.page_count(), gen_sample::SAMPLE_PAGES.len());
}
