//! Writes the text-heavy reference PDF the performance baselines are
//! recorded against — see `docs/perf-baseline.md`.
//!
//! ```text
//! cargo run --release -p gen-fixtures --example perf_reference
//! cargo run --release -p gen-fixtures --example perf_reference -- 400
//! cargo run --release -p gen-fixtures --example perf_reference -- 400 /tmp/reference.pdf
//! ```
//!
//! The first argument is the page count (default 1600), the second the
//! output path (default `tests/fixtures/large/perf_reference_<pages>pg.pdf`,
//! a directory the repository ignores).

use std::path::PathBuf;

use gen_fixtures::perf_reference::{default_output_path, generate_reference_pdf, DEFAULT_PAGES};

fn main() {
    let mut args = std::env::args().skip(1);
    let pages = match args.next() {
        None => DEFAULT_PAGES,
        Some(raw) => match raw.parse::<u32>() {
            Ok(pages) if pages > 0 => pages,
            _ => {
                eprintln!("Page count must be a positive integer, got {raw:?}");
                std::process::exit(2);
            }
        },
    };
    let out_path = args
        .next()
        .map_or_else(|| default_output_path(pages), PathBuf::from);
    if let Some(extra) = args.next() {
        eprintln!("Unexpected argument {extra:?}; usage: perf_reference [pages] [output]");
        std::process::exit(2);
    }

    match generate_reference_pdf(&out_path, pages) {
        Ok(path) => {
            let bytes = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
            println!("Wrote {pages} pages ({bytes} bytes) to {}", path.display());
        }
        Err(error) => {
            eprintln!("Failed to write {}: {error}", out_path.display());
            std::process::exit(1);
        }
    }
}
