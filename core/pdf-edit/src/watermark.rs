//! Writing a text watermark into a page's content (the "Add watermark" tool).
//!
//! The stamp is appended as a **new** content stream, the same way
//! [`crate::insert`] adds text, so nothing already on the page is decoded or
//! rewritten. Three things set it apart from an inserted text run:
//!
//! - it is centred in the page's visible box (`/CropBox`, else `/MediaBox`)
//!   and turned by the requested angle *as the reader sees it* — the page's
//!   `/Rotate` is added back in, because that entry turns the whole page for
//!   display and would otherwise turn the stamp with it;
//! - its opacity needs a graphics state of its own (`/ca`, `/CA`), registered
//!   under a fresh `/ExtGState` name;
//! - it is wrapped in `/Artifact << /Type /Pagination /Subtype /Watermark >>`
//!   marked content (PDF 1.7 §14.8.2.2), so screen readers skip it and text
//!   extraction can tell it from the page's own words.

use crate::edit::{format_number, literal_string};
use crate::encoding::winansi::{encode_winansi, standard_14_char_width};
use crate::error::EditError;
use crate::insert::{
    add_resource, append_to_content, correction_matrix, dereferenced_dict,
    ensure_standard_font_resource, matrix_operator, owned_resources_snapshot,
    standard_font_resource_name_in, INSERTED_BASE_FONT,
};
use crate::parse::matrix::Matrix;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use pdf_document::Watermark;

/// Half of Helvetica's cap height (718/1000 em): the offset from the
/// baseline to the visual middle of capital letters, which is what a
/// watermark — almost always capitals — is centred on.
const HALF_CAP_HEIGHT_EM: f64 = 0.359;

/// How deep `/Parent` inheritance is followed for `/CropBox`, `/MediaBox`
/// and `/Rotate`, matching the resource lookups in [`crate::insert`].
const MAX_INHERITANCE_DEPTH: usize = 32;

/// Whether `watermark`'s text can be painted at all — the one question
/// about a watermark that needs the font, and so the one the model cannot
/// answer. Checks against the face [`stamp_watermark`] paints with, so a
/// shell can refuse the text before it records the command.
pub fn check_watermark(watermark: &Watermark) -> Result<(), EditError> {
    encode(watermark).map(|_| ())
}

/// Paints `watermark` on the page `page_object`.
///
/// The text is encoded before anything is written, so a character Helvetica
/// cannot show leaves the document exactly as it was.
pub fn stamp_watermark(
    document: &mut Document,
    page_object: ObjectId,
    watermark: &Watermark,
) -> Result<(), EditError> {
    let codes = encode(watermark)?;
    let correction = correction_matrix(document, page_object)?;
    let placement = placement(document, page_object, watermark);

    let resources = owned_resources_snapshot(document, page_object);
    let font_name = standard_font_resource_name_in(document, &resources, INSERTED_BASE_FONT);
    let state_name = free_name(document, &resources, b"ExtGState", "GSVitela");

    ensure_standard_font_resource(document, page_object, &font_name, INSERTED_BASE_FONT)?;
    let opacity = Object::Real(watermark.opacity as f32);
    let state_id = document.add_object(dictionary! {
        "Type" => "ExtGState",
        "ca" => opacity.clone(),
        "CA" => opacity,
    });
    add_resource(
        document,
        page_object,
        b"ExtGState",
        &state_name,
        Object::Reference(state_id),
    )?;

    let color = watermark.color;
    let operators = format!(
        "\n/Artifact << /Type /Pagination /Subtype /Watermark >> BDC \
         q {} /{} gs {} {} {} rg BT /{} {} Tf {} Tm {} Tj ET Q EMC\n",
        matrix_operator(correction),
        state_name,
        format_number(f64::from(color.r) / 255.0),
        format_number(f64::from(color.g) / 255.0),
        format_number(f64::from(color.b) / 255.0),
        font_name,
        format_number(watermark.font_size),
        matrix_numbers(placement),
        String::from_utf8_lossy(&literal_string(&codes)),
    );

    append_to_content(document, page_object, operators.as_bytes())
}

fn encode(watermark: &Watermark) -> Result<Vec<u8>, EditError> {
    encode_winansi(&watermark.text).map_err(|character| EditError::EncodingGap {
        character,
        resource_font_name: INSERTED_BASE_FONT.to_string(),
    })
}

/// The text matrix that puts the middle of the stamp on the middle of the
/// visible box, turned so the reader sees `watermark.rotation_degrees`.
fn placement(document: &Document, page_object: ObjectId, watermark: &Watermark) -> Matrix {
    let size = watermark.font_size;
    let width: f64 = watermark
        .text
        .chars()
        .filter_map(|character| standard_14_char_width(INSERTED_BASE_FONT, character))
        .map(|thousandths| f64::from(thousandths) / 1000.0 * size)
        .sum();

    let [left, bottom, right, top] = visible_box(document, page_object);
    let (centre_x, centre_y) = ((left + right) / 2.0, (bottom + top) / 2.0);

    // `/Rotate` turns the page clockwise for display, which shows content
    // drawn at angle θ as θ − R; adding R back cancels it.
    let rotate = page_rotation(document, page_object) as f64;
    let (sin, cos) = (watermark.rotation_degrees + rotate).to_radians().sin_cos();

    Matrix::translate(-width / 2.0, -HALF_CAP_HEIGHT_EM * size)
        .then(Matrix::new(cos, sin, -sin, cos, 0.0, 0.0))
        .then(Matrix::translate(centre_x, centre_y))
}

fn matrix_numbers(matrix: Matrix) -> String {
    [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f]
        .map(format_number)
        .join(" ")
}

/// `/CropBox`, else `/MediaBox`, as `[left, bottom, right, top]`. A page
/// with neither (a malformed file) falls back to US Letter.
fn visible_box(document: &Document, page_object: ObjectId) -> [f64; 4] {
    [b"CropBox".as_slice(), b"MediaBox".as_slice()]
        .into_iter()
        .find_map(|key| {
            inherited(document, page_object, key).and_then(|object| rect(document, &object))
        })
        .unwrap_or([0.0, 0.0, 612.0, 792.0])
}

fn rect(document: &Document, object: &Object) -> Option<[f64; 4]> {
    let object = match object {
        Object::Reference(id) => document.get_object(*id).ok()?,
        other => other,
    };
    let Object::Array(items) = object else {
        return None;
    };
    let numbers: Vec<f64> = items
        .iter()
        .map(|item| item.as_float().ok().map(f64::from))
        .collect::<Option<_>>()?;
    let [x1, y1, x2, y2] = numbers.as_slice() else {
        return None;
    };
    Some([x1.min(*x2), y1.min(*y2), x1.max(*x2), y1.max(*y2)])
}

/// The page's `/Rotate`, normalised to `0..360`. A value that is not a
/// multiple of 90 is invalid per the spec and read as no rotation.
fn page_rotation(document: &Document, page_object: ObjectId) -> i64 {
    let degrees = inherited(document, page_object, b"Rotate")
        .and_then(|object| object.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    if degrees % 90 == 0 {
        degrees
    } else {
        0
    }
}

/// `key` on the page, or on the nearest `/Parent` that defines it.
fn inherited(document: &Document, page_object: ObjectId, key: &[u8]) -> Option<Object> {
    let mut current = document.get_dictionary(page_object).ok()?;
    for _ in 0..MAX_INHERITANCE_DEPTH {
        if let Ok(value) = current.get(key) {
            return Some(value.clone());
        }
        let Ok(Object::Reference(parent)) = current.get(b"Parent") else {
            return None;
        };
        current = document.get_dictionary(*parent).ok()?;
    }
    None
}

/// The first `{prefix}{n}` the page's `category` resources do not use.
fn free_name(document: &Document, resources: &Dictionary, category: &[u8], prefix: &str) -> String {
    let taken = resources
        .get(category)
        .ok()
        .and_then(|object| dereferenced_dict(document, object))
        .unwrap_or_default();
    (1_u32..)
        .map(|suffix| format!("{prefix}{suffix}"))
        .find(|candidate| taken.get(candidate.as_bytes()).is_err())
        .expect("a finite resource dictionary cannot occupy every u32 suffix")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{document_with_content, helvetica_resources};
    use crate::parse::read_page_object_content;
    use lopdf::{dictionary, Dictionary, Object};
    use pdf_document::{PageId, Rect, TextRun};

    const HELLO: &[u8] = b"BT /F1 12 Tf 72 700 Td (Hello) Tj ET";

    fn draft() -> Watermark {
        Watermark::diagonal(vec![PageId(0)], "DRAFT")
    }

    fn flat(text: &str) -> Watermark {
        Watermark {
            rotation_degrees: 0.0,
            ..Watermark::diagonal(vec![PageId(0)], text)
        }
    }

    fn runs(document: &Document, page: ObjectId) -> Vec<TextRun> {
        read_page_object_content(document, page, PageId(0))
            .expect("readable page")
            .text_runs
    }

    fn stamped_run(document: &Document, page: ObjectId, text: &str) -> TextRun {
        runs(document, page)
            .into_iter()
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("no run reads {text:?}"))
    }

    fn center(rect: Rect) -> (f64, f64) {
        (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    fn page_resources(document: &Document, page: ObjectId) -> Dictionary {
        crate::insert::owned_resources_snapshot(document, page)
    }

    fn resource_category(document: &Document, page: ObjectId, category: &[u8]) -> Dictionary {
        let resources = page_resources(document, page);
        match resources.get(category).expect("category present") {
            Object::Dictionary(dict) => dict.clone(),
            Object::Reference(id) => document.get_dictionary(*id).expect("dict").clone(),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn the_stamp_is_new_page_text_and_the_original_content_stays() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());

        stamp_watermark(&mut document, page, &draft()).expect("stampable");

        let texts: Vec<String> = runs(&document, page).into_iter().map(|r| r.text).collect();
        assert_eq!(texts, ["Hello", "DRAFT"]);
    }

    #[test]
    fn a_flat_stamp_is_centred_on_the_page() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());

        stamp_watermark(&mut document, page, &flat("DRAFT")).expect("stampable");

        let run = stamped_run(&document, page, "DRAFT");
        let (x, y) = center(run.bbox);
        assert!((x - 306.0).abs() < 1.0, "horizontal centre {x}");
        assert!(
            (y - 396.0).abs() < 0.5 * Watermark::DEFAULT_FONT_SIZE,
            "vertical centre {y}"
        );
        assert!(
            run.bbox.width > run.bbox.height,
            "flat text is wider than tall"
        );
    }

    #[test]
    fn the_crop_box_is_what_the_stamp_is_centred_in() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());
        document
            .get_dictionary_mut(page)
            .unwrap()
            .set("CropBox", vec![0.into(), 0.into(), 300.into(), 300.into()]);

        stamp_watermark(&mut document, page, &flat("X")).expect("stampable");

        let (x, _) = center(stamped_run(&document, page, "X").bbox);
        assert!((x - 150.0).abs() < 1.0, "horizontal centre {x}");
    }

    #[test]
    fn a_quarter_turn_stands_the_text_up() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());
        let upright = Watermark {
            rotation_degrees: 90.0,
            ..flat("DRAFT")
        };

        stamp_watermark(&mut document, page, &upright).expect("stampable");

        let run = stamped_run(&document, page, "DRAFT");
        assert!(run.bbox.height > run.bbox.width, "{:?}", run.bbox);
    }

    /// `/Rotate 90` turns the page clockwise for the reader. Text that must
    /// *read* flat therefore has to be written a quarter turn the other way
    /// in the page's own space — the parser reports that space, so the box
    /// comes back standing up.
    #[test]
    fn a_rotated_page_is_compensated_so_the_reader_sees_the_requested_angle() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());
        document.get_dictionary_mut(page).unwrap().set("Rotate", 90);

        stamp_watermark(&mut document, page, &flat("DRAFT")).expect("stampable");

        let run = stamped_run(&document, page, "DRAFT");
        assert!(run.bbox.height > run.bbox.width, "{:?}", run.bbox);
    }

    #[test]
    fn the_opacity_is_set_through_its_own_graphics_state() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());

        stamp_watermark(&mut document, page, &draft()).expect("stampable");

        let states = resource_category(&document, page, b"ExtGState");
        let (_, state) = states.iter().next().expect("one graphics state");
        let state = match state {
            Object::Dictionary(dict) => dict.clone(),
            Object::Reference(id) => document.get_dictionary(*id).unwrap().clone(),
            other => panic!("unexpected {other:?}"),
        };
        for key in [b"ca".as_slice(), b"CA".as_slice()] {
            let value = state.get(key).unwrap().as_float().unwrap();
            assert!((f64::from(value) - Watermark::DEFAULT_OPACITY).abs() < 1e-6);
        }
    }

    /// A page that already uses `F1` for another face must keep painting its
    /// own text in that face — the stamp picks a name of its own.
    #[test]
    fn an_existing_font_resource_is_never_overwritten() {
        let resources = dictionary! {
            "Font" => dictionary! {
                "F1" => dictionary! {
                    "Type" => "Font",
                    "Subtype" => "Type1",
                    "BaseFont" => "Times-Roman",
                    "Encoding" => "WinAnsiEncoding",
                },
            },
        };
        let (mut document, page) = document_with_content(HELLO, resources);

        stamp_watermark(&mut document, page, &draft()).expect("stampable");

        let fonts = resource_category(&document, page, b"Font");
        let f1 = match fonts.get(b"F1").unwrap() {
            Object::Dictionary(dict) => dict.clone(),
            Object::Reference(id) => document.get_dictionary(*id).unwrap().clone(),
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(
            f1.get(b"BaseFont").unwrap().as_name().unwrap(),
            b"Times-Roman"
        );
        assert_ne!(
            stamped_run(&document, page, "DRAFT").resource_font_name,
            "F1"
        );
    }

    #[test]
    fn the_stamp_is_marked_as_a_watermark_artifact() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());

        stamp_watermark(&mut document, page, &draft()).expect("stampable");

        let contents = document
            .get_dictionary(page)
            .unwrap()
            .get(b"Contents")
            .unwrap();
        let Object::Array(streams) = contents else {
            panic!("the stamp is appended as a second stream, got {contents:?}");
        };
        let last = streams.last().unwrap().as_reference().unwrap();
        let bytes = document
            .get_object(last)
            .unwrap()
            .as_stream()
            .unwrap()
            .content
            .clone();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/Artifact"), "{text}");
        assert!(text.contains("/Watermark"), "{text}");
    }

    #[test]
    fn text_helvetica_cannot_show_is_refused_before_the_page_is_touched() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());
        let before = document.clone();

        let error = stamp_watermark(&mut document, page, &flat("草稿")).unwrap_err();

        assert!(
            matches!(
                error,
                EditError::EncodingGap {
                    character: '草',
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(document.objects, before.objects);
    }

    #[test]
    fn checking_reports_the_same_gap_without_a_document() {
        assert!(check_watermark(&draft()).is_ok());
        assert!(matches!(
            check_watermark(&flat("café ✓")),
            Err(EditError::EncodingGap {
                character: '✓', ..
            })
        ));
    }
}
