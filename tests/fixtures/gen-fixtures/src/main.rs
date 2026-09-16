//! CLI entry point: regenerates the committed test corpora under
//! `tests/fixtures/encrypted/`, `tests/fixtures/signed/` and
//! `tests/fixtures/compress/`.
//!
//! Usage:
//!
//! ```text
//! cargo run -p gen-fixtures                  # every corpus
//! cargo run -p gen-fixtures -- compress      # one of them
//! ```
//!
//! **Naming one matters**, which is why the argument exists. The signed
//! corpus mints fresh random keys and certificates on every run, so
//! regenerating it always rewrites four files whether or not anything about
//! them changed — and someone regenerating the compression corpus after
//! editing its generator would otherwise hand a reviewer four unrelated,
//! unexplainable binary diffs.

use std::path::{Path, PathBuf};

/// The corpora this binary can write, by the name it answers to.
const CORPORA: [&str; 3] = ["encrypted", "signed", "compress"];

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

/// Runs one corpus, or exits with its error.
fn generate(name: &str, fixtures_root: &Path) {
    let out_dir = fixtures_root.join(name);
    let written = match name {
        "encrypted" => gen_fixtures::generate_all(&out_dir),
        "signed" => gen_fixtures::signed::generate_signed_corpus(&out_dir),
        "compress" => gen_fixtures::compress::generate_compress_corpus(&out_dir),
        _ => unreachable!("the caller checked the name against CORPORA"),
    };

    match written {
        Ok(paths) => report(name, &out_dir, &paths),
        Err(err) => {
            eprintln!("Failed to generate {name} fixtures: {err}");
            std::process::exit(1);
        }
    }
}

fn main() {
    let fixtures_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");

    let requested: Vec<String> = std::env::args().skip(1).collect();
    if let Some(unknown) = requested
        .iter()
        .find(|name| !CORPORA.contains(&name.as_str()))
    {
        eprintln!("Unknown corpus {unknown:?}; expected one of {CORPORA:?}");
        std::process::exit(2);
    }

    for name in CORPORA {
        if requested.is_empty() || requested.iter().any(|wanted| wanted == name) {
            generate(name, &fixtures_root);
        }
    }
}
