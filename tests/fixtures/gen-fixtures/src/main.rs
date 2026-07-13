//! CLI entry point: regenerates the committed encrypted-PDF corpus under
//! `tests/fixtures/encrypted/`.
//!
//! Usage: `cargo run -p gen-fixtures`

use std::path::Path;

fn main() {
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../encrypted");
    match gen_fixtures::generate_all(&out_dir) {
        Ok(paths) => {
            println!(
                "Generated {} fixture(s) into {}:",
                paths.len(),
                out_dir.display()
            );
            for path in paths {
                println!("  {}", path.display());
            }
        }
        Err(err) => {
            eprintln!("Failed to generate fixtures: {err}");
            std::process::exit(1);
        }
    }
}
