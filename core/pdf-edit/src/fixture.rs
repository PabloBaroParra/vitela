//! Minimal in-memory PDFs for this crate's own tests.
//!
//! Deliberately hand-built rather than loaded from `tests/fixtures`: these
//! exercise the interpreter against exact, known content-stream bytes. The
//! real-world files (including one produced by an external tool) arrive with
//! T-159 and belong to the round-trip tests, not to unit tests of the
//! tokenizer and interpreter.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

/// A one-page document whose `/Contents` is a single stream.
pub fn document_with_content(content: &[u8], resources: Dictionary) -> (Document, ObjectId) {
    document_with_streams(&[content], resources)
}

/// A one-page document whose `/Contents` is an array of streams — the case
/// where graphics state set in one stream must carry into the next.
pub fn document_with_streams(contents: &[&[u8]], resources: Dictionary) -> (Document, ObjectId) {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();

    let stream_ids: Vec<Object> = contents
        .iter()
        .map(|content| {
            document
                .add_object(Stream::new(dictionary! {}, content.to_vec()))
                .into()
        })
        .collect();
    let contents_entry = if stream_ids.len() == 1 {
        stream_ids[0].clone()
    } else {
        Object::Array(stream_ids)
    };

    let resources = with_indirect_streams(&mut document, resources);
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => contents_entry,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => resources,
    });

    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1,
            "Kids" => vec![page_id.into()],
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);

    (document, page_id)
}

/// Promotes any stream sitting directly inside a resource category to an
/// indirect object.
///
/// The tests build resource dictionaries before there is a document to add
/// objects to, but a PDF stream is only ever an indirect object — and code
/// that swaps an image's bytes needs an object id to address. Building
/// fixtures that break that rule would test against files no reader
/// produces.
fn with_indirect_streams(document: &mut Document, resources: Dictionary) -> Dictionary {
    let mut fixed = Dictionary::new();

    for (category, value) in resources.iter() {
        let Object::Dictionary(entries) = value else {
            fixed.set(
                String::from_utf8_lossy(category).into_owned(),
                value.clone(),
            );
            continue;
        };

        let mut fixed_entries = Dictionary::new();
        for (name, entry) in entries.iter() {
            let name = String::from_utf8_lossy(name).into_owned();
            match entry {
                Object::Stream(stream) => {
                    let id = document.add_object(stream.clone());
                    fixed_entries.set(name, Object::Reference(id));
                }
                other => fixed_entries.set(name, other.clone()),
            }
        }
        fixed.set(
            String::from_utf8_lossy(category).into_owned(),
            Object::Dictionary(fixed_entries),
        );
    }

    fixed
}

/// `/F1` is Helvetica with WinAnsi codes and no `/Widths` — the standard-14
/// case, where advances come from this crate's fallback.
pub fn helvetica_resources() -> Dictionary {
    dictionary! {
        "Font" => dictionary! {
            "F1" => dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => "Helvetica",
                "Encoding" => "WinAnsiEncoding",
            },
        },
    }
}

/// `/Im1` is a 2x2 image XObject.
pub fn image_resources() -> Dictionary {
    dictionary! {
        "XObject" => dictionary! {
            "Im1" => Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => 2,
                    "Height" => 2,
                    "ColorSpace" => "DeviceRGB",
                    "BitsPerComponent" => 8,
                },
                vec![0u8; 12],
            ),
        },
    }
}

/// A page carrying both a font and an image resource.
pub fn text_and_image_resources() -> Dictionary {
    let mut resources = helvetica_resources();
    let Ok(Object::Dictionary(xobjects)) = image_resources().get(b"XObject").cloned() else {
        unreachable!("image_resources always has an XObject dictionary");
    };
    resources.set("XObject", xobjects);
    resources
}
