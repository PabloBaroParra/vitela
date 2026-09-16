//! T-191's measurement, kept as a test so the numbers in the ficha can be
//! re-derived rather than believed.
//!
//! `docs/batch-compress.md` fact 4 is written as a question, not a claim:
//! *today's save path rewrites a document that arrived with object streams as
//! loose objects plus a classic cross-reference table, and that may inflate
//! it — measure, do not assume.* This file is the measurement. It walks the
//! corpus and, for each file, reports three sizes:
//!
//! 1. what the user handed us,
//! 2. what today's `pdf-save` full rewrite produces (`load_mem` + `save_to`,
//!    `strategy.rs:451`),
//! 3. what [`pdf_compress::compress`] produces.
//!
//! Run the table yourself with:
//!
//! ```text
//! cargo test -p pdf-compress --test measure -- --nocapture
//! ```
//!
//! The one assertion here is fact 4 itself. The guardians that protect the
//! feature live next door in `corpus.rs`.

mod common;

use common::{percent, read, CORPUS};
use pdf_compress::{compress, CompressPreset, SignedDocuments};

/// What today's `pdf-save` full rewrite does to these bytes: load them, write
/// them straight back out with `save_to`. Fact 2's "most expensive path in
/// lopdf" — every object serialised individually under a classic xref table.
///
/// `None` when the document is encrypted, because this path does not merely
/// inflate one: `load_mem` on an encrypted file returns a handle holding
/// nothing but its `/Encrypt` dictionary, so re-serialising it writes out an
/// *empty* PDF. Reporting that as a size would put a spectacular saving in
/// the table for a file that was destroyed.
fn todays_full_rewrite(input: &[u8]) -> Option<usize> {
    let mut document = lopdf::Document::load_mem(input).ok()?;
    if document.is_encrypted() {
        return None;
    }

    let mut bytes = Vec::new();
    document.save_to(&mut bytes).ok()?;
    Some(bytes.len())
}

/// Fact 4's actual question, which no fixture in this repository can answer:
/// *what does today's save path do to a document that arrived **with** object
/// streams?* Every committed fixture was written with a classic
/// cross-reference table, so this builds the missing case — takes one of them
/// and re-writes it the modern way first — and measures the round trip
/// through `save_to`.
///
/// Returns `(packed input size, size after today's full rewrite)`.
fn modern_document_through_todays_rewrite(input: &[u8]) -> Option<(usize, usize)> {
    let mut document = lopdf::Document::load_mem(input).ok()?;
    if document.is_encrypted() {
        return None;
    }

    let mut packed = Vec::new();
    document
        .save_with_options(
            &mut packed,
            lopdf::SaveOptions {
                use_object_streams: true,
                use_xref_streams: true,
                ..lopdf::SaveOptions::default()
            },
        )
        .ok()?;

    let rewritten = todays_full_rewrite(&packed)?;
    Some((packed.len(), rewritten))
}

/// Fact 4, measured. Prints the table that goes into `docs/batch-compress.md`.
#[test]
fn measure_todays_rewrite_against_the_structural_pass() {
    println!(
        "\n{:<52} {:>10} {:>14} {:>14} {:>9}",
        "fixture", "original", "save_to today", "compress()", "vs orig"
    );

    for fixture in CORPUS {
        let Some(input) = read(fixture) else {
            println!("{:<52} {:>10}", fixture.path, "(not generated)");
            continue;
        };

        let rewritten = todays_full_rewrite(&input);
        let compressed = compress(
            &input,
            CompressPreset::Lossless,
            SignedDocuments::LeaveAlone,
        )
        .unwrap_or_else(|err| panic!("{} could not be compressed: {err}", fixture.path));

        let rewritten_cell = match rewritten {
            Some(size) => format!("{size} ({:+.1}%)", percent(input.len(), size)),
            None => "destroys it".to_string(),
        };

        println!(
            "{:<52} {:>10} {:>14} {:>14} {:>8.1}%",
            fixture.path,
            input.len(),
            rewritten_cell,
            compressed.bytes().len(),
            percent(input.len(), compressed.bytes().len()),
        );
        println!(
            "{:<52} — {:?}, {} streams flated, {} objects dropped{}",
            format!("  ({})", fixture.what),
            compressed.report().outcome(),
            compressed.report().work().streams_recompressed,
            compressed.report().work().objects_dropped,
            if compressed.report().refusals().is_empty() {
                String::new()
            } else {
                format!(", refused: {:?}", compressed.report().refusals())
            }
        );
    }

    println!(
        "\nfact 4 — a document that arrived ALREADY packed, put through today's save_to:\n{:<52} {:>10} {:>14} {:>9}",
        "fixture (repacked first)", "packed", "save_to today", "delta"
    );
    for fixture in CORPUS {
        // Protected documents are excluded rather than shown: the packing
        // step this row starts from is itself what drops their signature, so
        // the "packed" figure would not describe the same document and the
        // delta would be theatre.
        if fixture.protected {
            continue;
        }
        let Some(input) = read(fixture) else {
            continue;
        };
        let Some((packed, rewritten)) = modern_document_through_todays_rewrite(&input) else {
            continue;
        };

        println!(
            "{:<52} {:>10} {:>14} {:>8.1}%",
            fixture.path,
            packed,
            rewritten,
            percent(packed, rewritten)
        );
    }
    println!();
}

/// Fact 4 as an assertion rather than a printout: today's full rewrite really
/// does inflate a document that arrived packed. This is the regression the
/// compression feature closes on its way past — not a claim in the ficha, a
/// test that fails if it ever stops being true.
#[test]
fn todays_full_rewrite_inflates_a_document_that_arrived_packed() {
    let input = read(&CORPUS[1]).expect("the content-edit fixture is committed");

    let (packed, rewritten) =
        modern_document_through_todays_rewrite(&input).expect("it is neither encrypted nor broken");

    assert!(
        rewritten > packed,
        "today's save_to turned a {packed}-byte packed document into {rewritten} bytes; \
         if this is no longer true, fact 4 in docs/batch-compress.md is stale"
    );
}
