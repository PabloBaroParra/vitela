//! The compression corpus (T-197) must be what it claims to be, and must
//! still be what is committed.
//!
//! `core/pdf-compress/tests/` asks what the compressor *does* to these files.
//! This asks the question one step earlier: are the files the ones those
//! tests think they are reading? A fixture that quietly stopped carrying a
//! soft mask, or stopped being sampled above a preset's ceiling, would leave
//! every guardian next door passing while proving nothing — the exact failure
//! mode T-197 was written to remove, reintroduced from below.
//!
//! Unlike the signed corpus, this generator **is** byte-reproducible: nothing
//! in it is random, so the committed files can be pinned to it. That check is
//! the last test in this file, and a failure there means the generator was
//! edited without regenerating — `cargo run -p gen-fixtures -- compress`.

use std::path::{Path, PathBuf};

use gen_fixtures::compress::{build, generate_compress_corpus, serialise, CORPUS};
use lopdf::{Document, Object};

fn unique_temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "gen-fixtures-compress-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

/// Where the committed corpus lives, from this crate's manifest.
fn committed_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("compress")
}

/// The bytes the generator produces for `file_name`, without touching disk.
fn generated(file_name: &str) -> Vec<u8> {
    serialise(build(file_name).expect("the fixture builds"), file_name)
        .expect("the fixture serialises")
}

/// Every image XObject in `document`, as `(width, height, filter, has_smask)`.
fn images(document: &Document) -> Vec<(i64, i64, Option<String>, bool)> {
    document
        .objects
        .values()
        .filter_map(|object| {
            let stream = object.as_stream().ok()?;
            (stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image")).then(
                || {
                    let side =
                        |key: &[u8]| stream.dict.get(key).and_then(Object::as_i64).unwrap_or(-1);
                    (
                        side(b"Width"),
                        side(b"Height"),
                        stream
                            .dict
                            .get(b"Filter")
                            .and_then(Object::as_name)
                            .ok()
                            .map(|name| String::from_utf8_lossy(name).into_owned()),
                        stream.dict.has(b"SMask"),
                    )
                },
            )
        })
        .collect()
}

#[test]
fn every_fixture_is_written_and_loads_as_a_pdf() {
    let out_dir = unique_temp_dir("load");
    let written = generate_compress_corpus(&out_dir).expect("the corpus generates");

    assert_eq!(written.len(), CORPUS.len(), "one file per corpus entry");
    for path in &written {
        let document = Document::load(path)
            .unwrap_or_else(|err| panic!("{} must load as a PDF: {err}", path.display()));
        assert!(
            !document.get_pages().is_empty(),
            "{} has no pages",
            path.display()
        );
    }

    let _ = std::fs::remove_dir_all(&out_dir);
}

/// The scan's whole reason for existing: one lossy image, sampled far above
/// every preset's ceiling.
///
/// 1700 samples across 612 points of paper is 200 effective dpi — above
/// `Balanced`'s 150 and more than twice `Small`'s 96. Drop either number and
/// the resampler has nothing to do on the only file in the repository that
/// was added to give it something.
#[test]
fn the_scan_is_one_lossy_image_above_every_ceiling() {
    let document = build("scan_200dpi.pdf").expect("the scan builds");

    assert_eq!(
        images(&document),
        vec![(1700, 2200, Some("DCTDecode".to_string()), false)],
        "the scan must be exactly one DCTDecode image at 200 dpi over US Letter"
    );
    assert_eq!(document.get_pages().len(), 1);
}

/// One XObject, two pages, two sizes — which is what makes the *largest
/// placement governs* rule checkable at all.
///
/// The property is that both pages name the **same** object. Two copies of
/// one image drawn at two sizes would look identical in a viewer and would
/// take the rule out of play entirely, since each copy would then have one
/// placement of its own.
#[test]
fn the_reused_image_is_one_object_painted_on_both_pages() {
    let document = build("reused_image_two_scales.pdf").expect("it builds");

    assert_eq!(images(&document).len(), 1, "one image object, not two");
    assert_eq!(document.get_pages().len(), 2);

    let referenced: Vec<Object> = document
        .get_pages()
        .values()
        .map(|page_id| {
            let resources = document
                .get_dictionary(*page_id)
                .and_then(|page| page.get(b"Resources"))
                .and_then(Object::as_reference)
                .expect("the page names a resource dictionary");
            document
                .get_dictionary(resources)
                .and_then(|dictionary| dictionary.get(b"XObject"))
                .and_then(Object::as_dict)
                .and_then(|xobjects| xobjects.get(b"Im0"))
                .expect("the page names /Im0")
                .clone()
        })
        .collect();

    assert_eq!(referenced.len(), 2);
    assert_eq!(
        referenced[0].as_reference().ok(),
        referenced[1].as_reference().ok(),
        "the two pages must paint the same object, or neither placement governs \
         anything the other one does not"
    );
}

/// Real transparency: a lossy photograph whose `/SMask` is a separate,
/// same-sized grey image.
///
/// Same sized on purpose. A mask already below the preset's ceiling would be
/// left byte-identical whatever happened to its parent, and the fixture would
/// then say nothing about the thing that is easy to get wrong — that the mask
/// comes down with the image and goes on covering the same paper.
#[test]
fn the_transparency_fixture_carries_a_real_soft_mask() {
    let document = build("transparency_smask.pdf").expect("it builds");
    let mut found = images(&document);
    found.sort();

    assert_eq!(
        found,
        vec![
            (600, 600, Some("DCTDecode".to_string()), true),
            (600, 600, Some("FlateDecode".to_string()), false),
        ],
        "the fixture must be a lossy photograph plus a flate mask of the same size"
    );
}

/// The pure-vector row, stated as an absence.
#[test]
fn the_vector_fixture_holds_no_image_at_all() {
    let document = build("vector_only.pdf").expect("it builds");

    assert!(
        images(&document).is_empty(),
        "a document added to prove the image stage finds nothing has an image in it"
    );
    assert_eq!(document.get_pages().len(), 3);
}

/// The no-slack row is only a no-slack row if it was actually written the
/// packed way.
///
/// It is the one fixture whose *write format* is its content: loose objects
/// under a classic cross-reference table would leave the repack plenty to do,
/// and `NoGain` would stop being the honest answer to it.
#[test]
fn the_packed_fixture_really_is_packed() {
    let bytes = generated("already_packed.pdf");

    assert!(
        bytes.windows(6).any(|window| window == b"ObjStm"),
        "the packed fixture carries no object stream"
    );

    let document = Document::load_mem(&bytes).expect("it loads");
    assert!(
        matches!(
            document.reference_table.cross_reference_type,
            lopdf::xref::XrefType::CrossReferenceStream
        ),
        "the packed fixture was written with a classic cross-reference table"
    );
    // Every *content* stream, which is what the structural pass flates. The
    // cross-reference stream is excluded because it is not content: lopdf
    // writes it unfiltered, and it is rebuilt from scratch by whoever saves
    // the file next, so its filter says nothing about whether this document
    // still has slack in it.
    assert!(
        document.objects.values().all(|object| match object {
            Object::Stream(stream) =>
                stream.dict.has(b"Filter")
                    || stream.dict.get(b"Type").and_then(Object::as_name).ok()
                        == Some(b"XRef".as_slice()),
            _ => true,
        }),
        "a content stream in the packed fixture arrived unfiltered, which is \
         slack the repack would correctly take"
    );
}

/// Nothing in this generator is random, so it must produce the same bytes
/// twice — the property the check below depends on.
#[test]
fn the_generator_is_byte_reproducible() {
    for fixture in CORPUS {
        assert_eq!(
            generated(fixture.file_name),
            generated(fixture.file_name),
            "{} differs between two runs of the same generator",
            fixture.file_name
        );
    }
}

/// The committed files are the ones this generator produces.
///
/// The check that keeps `tests/fixtures/compress/` honest. Editing a builder
/// without regenerating leaves the repository holding one corpus and the
/// source describing another, and every test that reads a file would go on
/// passing against the stale one.
#[test]
fn the_committed_corpus_matches_the_generator() {
    for fixture in CORPUS {
        let path = committed_dir().join(fixture.file_name);
        let committed = std::fs::read(&path)
            .unwrap_or_else(|err| panic!("{} must be committed: {err}", path.display()));

        assert_eq!(
            committed,
            generated(fixture.file_name),
            "{} is not what the generator produces today — run \n             `cargo run -p gen-fixtures -- compress`",
            fixture.file_name
        );
    }
}
