//! CLI entry point: regenerates the committed sample document under
//! `assets/sample/`, which every platform shell packages.
//!
//! Usage: `cargo run -p gen-sample`
//!
//! Output is byte-reproducible, so a clean run on an unchanged tree leaves
//! `git status` clean.

use std::path::Path;

fn main() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out_path = repo_root
        .join("assets/sample")
        .join(gen_sample::SAMPLE_FILE_NAME);

    match gen_sample::generate_sample(&out_path) {
        Ok(path) => println!("Generated sample document: {}", path.display()),
        Err(error) => {
            eprintln!("Failed to generate the sample document: {error}");
            std::process::exit(1);
        }
    }
}
