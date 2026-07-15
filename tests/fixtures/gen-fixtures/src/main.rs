//! CLI entry point: regenerates the committed test corpora under
//! `tests/fixtures/encrypted/` and `tests/fixtures/signed/`.
//!
//! Usage: `cargo run -p gen-fixtures`

use std::path::{Path, PathBuf};

fn report(label: &str, out_dir: &Path, paths: &[PathBuf]) {
    println!(
        "Generated {} {label} fixture(s) into {}:",
        paths.len(),
        out_dir.display()
    );
    for path in paths {
        println!("  {}", path.display());
    }
}

fn main() {
    let fixtures_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");

    let encrypted_dir = fixtures_root.join("encrypted");
    match gen_fixtures::generate_all(&encrypted_dir) {
        Ok(paths) => report("encrypted", &encrypted_dir, &paths),
        Err(err) => {
            eprintln!("Failed to generate encrypted fixtures: {err}");
            std::process::exit(1);
        }
    }

    let signed_dir = fixtures_root.join("signed");
    match gen_fixtures::signed::generate_signed_corpus(&signed_dir) {
        Ok(paths) => report("signed", &signed_dir, &paths),
        Err(err) => {
            eprintln!("Failed to generate signed fixtures: {err}");
            std::process::exit(1);
        }
    }
}
