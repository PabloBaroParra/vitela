//! Integration test (T-004, TDD): the fixture generator must produce an
//! encrypted-PDF corpus (RC4/AES x user/owner password) that:
//! - is detected as encrypted by lopdf,
//! - opens successfully with either the correct user password or the
//!   correct owner password (spec.md "Open Password-Protected PDF" —
//!   correct-password scenario),
//! - fails cleanly (no panic) with a wrong password (wrong-password scenario).

use std::path::PathBuf;

use gen_fixtures::{generate_all, CORPUS};
use lopdf::{Document, LoadOptions};

fn unique_temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "gen-fixtures-test-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn generates_full_corpus_of_expected_size() {
    let out_dir = unique_temp_dir("size");
    let written = generate_all(&out_dir).expect("generate_all should succeed");

    assert_eq!(written.len(), CORPUS.len(), "one file per corpus spec");
    for path in &written {
        assert!(path.exists(), "fixture file must exist: {}", path.display());
        let metadata = std::fs::metadata(path).unwrap();
        assert!(
            metadata.len() > 0,
            "fixture file must not be empty: {}",
            path.display()
        );
    }

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn every_fixture_is_detected_as_encrypted() {
    let out_dir = unique_temp_dir("detect");
    generate_all(&out_dir).unwrap();

    for spec in CORPUS {
        let path = out_dir.join(spec.file_name);
        let doc = Document::load(&path).expect("fixture must load as a valid PDF");
        assert!(
            doc.is_encrypted(),
            "{} must be detected as encrypted",
            spec.file_name
        );
    }

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn every_fixture_opens_with_correct_user_password() {
    let out_dir = unique_temp_dir("user-pw");
    generate_all(&out_dir).unwrap();

    for spec in CORPUS {
        let path = out_dir.join(spec.file_name);
        let doc =
            Document::load_with_options(&path, LoadOptions::with_password(spec.user_password))
                .unwrap_or_else(|e| {
                    panic!(
                        "{} should open with correct user password: {e}",
                        spec.file_name
                    )
                });
        assert!(
            !doc.get_pages().is_empty(),
            "decrypted document must expose its page(s)"
        );
    }

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn every_fixture_opens_with_correct_owner_password() {
    let out_dir = unique_temp_dir("owner-pw");
    generate_all(&out_dir).unwrap();

    for spec in CORPUS {
        let path = out_dir.join(spec.file_name);
        let doc =
            Document::load_with_options(&path, LoadOptions::with_password(spec.owner_password))
                .unwrap_or_else(|e| {
                    panic!(
                        "{} should open with correct owner password: {e}",
                        spec.file_name
                    )
                });
        assert!(
            !doc.get_pages().is_empty(),
            "decrypted document must expose its page(s)"
        );
    }

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn every_fixture_rejects_wrong_password_cleanly() {
    let out_dir = unique_temp_dir("wrong-pw");
    generate_all(&out_dir).unwrap();

    for spec in CORPUS {
        let path = out_dir.join(spec.file_name);
        let result = Document::load_with_options(
            &path,
            LoadOptions::with_password("definitely-the-wrong-password"),
        );
        assert!(
            result.is_err(),
            "{} must reject a wrong password, not silently succeed",
            spec.file_name
        );
    }

    let _ = std::fs::remove_dir_all(&out_dir);
}
