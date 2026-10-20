//! Writing a watermark into page content (the "Add watermark" tool).
//!
//! The stamp is added as a **new** content stream, the same way
//! [`crate::insert`] adds text and images, so nothing already on the page is
//! decoded or rewritten. Four things set it apart from inserted content:
//!
//! - it is centred in the page's visible box (`/CropBox`, else `/MediaBox`)
//!   and turned by the requested angle *as the reader sees it* — the page's
//!   `/Rotate` is added back in, because that entry turns the whole page for
//!   display and would otherwise turn the stamp with it;
//! - its opacity needs a graphics state of its own (`/ca`, `/CA`), registered
//!   under a fresh `/ExtGState` name;
//! - it is wrapped in `/Artifact << /Type /Pagination /Subtype /Watermark >>`
//!   marked content (PDF 1.7 §14.8.2.2), so screen readers skip it and text
//!   extraction can tell it from the page's own words;
//! - a background ([`WatermarkLayer::UnderContent`]) is the **first** stream
//!   instead of the last, so the page's own content paints over it.
//!
//! A watermark covers many pages at once, so everything a page does not
//! change — the encoded text, the decoded picture, the graphics state — is
//! built once and shared: a logo on 300 pages is one image object, not 300.

use crate::edit::{format_number, literal_string};
use crate::encoding::winansi::{encode_winansi, standard_14_char_width};
use crate::error::EditError;
use crate::insert::{
    add_resource, append_to_content, correction_matrix, dereferenced_dict,
    ensure_standard_font_resource, image_xobject, matrix_operator, owned_resources_snapshot,
    prepend_to_content, standard_font_resource_name_in, ImageXObject, INSERTED_BASE_FONT,
};
use crate::parse::matrix::Matrix;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use pdf_document::{Color, Watermark, WatermarkLayer, WatermarkMark};

/// Half of Helvetica's cap height (718/1000 em): the offset from the
/// baseline to the visual middle of capital letters, which is what a
/// watermark — almost always capitals — is centred on.
const HALF_CAP_HEIGHT_EM: f64 = 0.359;

/// How deep `/Parent` inheritance is followed for `/CropBox`, `/MediaBox`
/// and `/Rotate`, matching the resource lookups in [`crate::insert`].
const MAX_INHERITANCE_DEPTH: usize = 32;

/// Whether `watermark`'s mark can be painted at all — the one question about
/// a watermark the model cannot answer: text needs the font, a picture needs
/// decoding. Checks exactly what [`stamp_watermark`] would, so a shell can
/// refuse the stamp before it records the command.
pub fn check_watermark(watermark: &Watermark) -> Result<(), EditError> {
    Prepared::new(watermark).map(|_| ())
}

/// Paints `watermark` on every page in `page_objects`.
///
/// The mark is encoded or decoded before anything is written, so a character
/// Helvetica cannot show, or bytes that are not a picture, leave the document
/// exactly as it was.
pub fn stamp_watermark(
    document: &mut Document,
    page_objects: &[ObjectId],
    watermark: &Watermark,
) -> Result<(), EditError> {
    let prepared = Prepared::new(watermark)?;
    if page_objects.is_empty() {
        return Ok(());
    }

    let opacity = Object::Real(watermark.opacity as f32);
    let state_id = document.add_object(dictionary! {
        "Type" => "ExtGState",
        "ca" => opacity.clone(),
        "CA" => opacity,
    });
    let shared = prepared.register(document);

    for &page_object in page_objects {
        stamp_page(document, page_object, watermark, &shared, state_id)?;
    }
    Ok(())
}

/// The part of a mark that is the same on every page, before anything is
/// written.
enum Prepared<'a> {
    Text {
        codes: Vec<u8>,
        text: &'a str,
        font_size: f64,
        color: Color,
    },
    Image {
        built: ImageXObject,
        scale: f64,
    },
}

/// [`Prepared`] after its shared objects are in the document.
enum Shared<'a> {
    Text {
        codes: Vec<u8>,
        text: &'a str,
        font_size: f64,
        color: Color,
    },
    Image {
        image: ObjectId,
        /// The picture's pixel aspect, width over height.
        aspect: f64,
        scale: f64,
    },
}

impl<'a> Prepared<'a> {
    fn new(watermark: &'a Watermark) -> Result<Self, EditError> {
        match &watermark.mark {
            WatermarkMark::Text {
                text,
                font_size,
                color,
            } => {
                let codes = encode_winansi(text).map_err(|character| EditError::EncodingGap {
                    character,
                    resource_font_name: INSERTED_BASE_FONT.to_string(),
                })?;
                Ok(Prepared::Text {
                    codes,
                    text,
                    font_size: *font_size,
                    color: *color,
                })
            }
            WatermarkMark::Image { bytes, scale } => Ok(Prepared::Image {
                built: image_xobject(bytes)?,
                scale: *scale,
            }),
        }
    }

    fn register(self, document: &mut Document) -> Shared<'a> {
        match self {
            Prepared::Text {
                codes,
                text,
                font_size,
                color,
            } => Shared::Text {
                codes,
                text,
                font_size,
                color,
            },
            Prepared::Image { built, scale } => {
                let dimension = |key: &[u8]| {
                    built
                        .image
                        .dict
                        .get(key)
                        .and_then(Object::as_i64)
                        .map_or(1.0, |pixels| pixels.max(1) as f64)
                };
                let aspect = dimension(b"Width") / dimension(b"Height");

                let mut image = built.image;
                if let Some(smask) = built.smask {
                    let smask_id = document.add_object(smask);
                    image.dict.set("SMask", Object::Reference(smask_id));
                }
                Shared::Image {
                    image: document.add_object(image),
                    aspect,
                    scale,
                }
            }
        }
    }
}

fn stamp_page(
    document: &mut Document,
    page_object: ObjectId,
    watermark: &Watermark,
    shared: &Shared<'_>,
    state_id: ObjectId,
) -> Result<(), EditError> {
    // A background runs in the page's initial state; a stamp on top has to
    // undo whatever transform the page's own streams leave behind.
    let correction = match watermark.layer {
        WatermarkLayer::OverContent => correction_matrix(document, page_object)?,
        WatermarkLayer::UnderContent => Matrix::IDENTITY,
    };
    let frame = PageFrame::of(document, page_object);

    let resources = owned_resources_snapshot(document, page_object);
    let state_name = free_name(document, &resources, b"ExtGState", "GSVitela");
    add_resource(
        document,
        page_object,
        b"ExtGState",
        &state_name,
        Object::Reference(state_id),
    )?;

    let paint = match shared {
        Shared::Text {
            codes,
            text,
            font_size,
            color,
        } => {
            let font_name =
                standard_font_resource_name_in(document, &resources, INSERTED_BASE_FONT);
            ensure_standard_font_resource(document, page_object, &font_name, INSERTED_BASE_FONT)?;
            let placement = frame.text_placement(text, *font_size, watermark.rotation_degrees);
            format!(
                "{} {} {} rg BT /{} {} Tf {} Tm {} Tj ET",
                format_number(f64::from(color.r) / 255.0),
                format_number(f64::from(color.g) / 255.0),
                format_number(f64::from(color.b) / 255.0),
                font_name,
                format_number(*font_size),
                matrix_numbers(placement),
                String::from_utf8_lossy(&literal_string(codes)),
            )
        }
        Shared::Image {
            image,
            aspect,
            scale,
        } => {
            let image_name = free_name(document, &resources, b"XObject", "ImVitela");
            add_resource(
                document,
                page_object,
                b"XObject",
                &image_name,
                Object::Reference(*image),
            )?;
            let placement = frame.image_placement(*aspect, *scale, watermark.rotation_degrees);
            format!("{} /{} Do", matrix_operator(placement), image_name)
        }
    };

    let operators = format!(
        "\n/Artifact << /Type /Pagination /Subtype /Watermark >> BDC \
         q {} /{} gs {} Q EMC\n",
        matrix_operator(correction),
        state_name,
        paint,
    );

    match watermark.layer {
        WatermarkLayer::OverContent => {
            append_to_content(document, page_object, operators.as_bytes())
        }
        WatermarkLayer::UnderContent => {
            prepend_to_content(document, page_object, operators.as_bytes())
        }
    }
}

/// The page geometry a stamp is centred and turned in.
struct PageFrame {
    /// `[left, bottom, right, top]` of the visible box, in page space.
    visible: [f64; 4],
    /// The page's `/Rotate`, `0`, `90`, `180` or `270`.
    rotate: i64,
}

impl PageFrame {
    fn of(document: &Document, page_object: ObjectId) -> Self {
        Self {
            visible: visible_box(document, page_object),
            rotate: page_rotation(document, page_object),
        }
    }

    fn centre(&self) -> (f64, f64) {
        let [left, bottom, right, top] = self.visible;
        ((left + right) / 2.0, (bottom + top) / 2.0)
    }

    /// The turn, in page space, that the reader sees as `degrees`.
    ///
    /// `/Rotate` turns the page clockwise for display, which shows content
    /// drawn at angle θ as θ − R; adding R back cancels it.
    fn turn(&self, degrees: f64) -> Matrix {
        let (sin, cos) = (degrees + self.rotate as f64).to_radians().sin_cos();
        Matrix::new(cos, sin, -sin, cos, 0.0, 0.0)
    }

    /// The visible box's width and height **as the reader sees them**: a
    /// quarter turn swaps them.
    fn reader_size(&self) -> (f64, f64) {
        let [left, bottom, right, top] = self.visible;
        let (width, height) = (right - left, top - bottom);
        if self.rotate % 180 == 0 {
            (width, height)
        } else {
            (height, width)
        }
    }

    /// The text matrix that puts the middle of the text on the middle of the
    /// visible box, turned so the reader sees `degrees`.
    fn text_placement(&self, text: &str, size: f64, degrees: f64) -> Matrix {
        let width: f64 = text
            .chars()
            .filter_map(|character| standard_14_char_width(INSERTED_BASE_FONT, character))
            .map(|thousandths| f64::from(thousandths) / 1000.0 * size)
            .sum();
        let (centre_x, centre_y) = self.centre();

        Matrix::translate(-width / 2.0, -HALF_CAP_HEIGHT_EM * size)
            .then(self.turn(degrees))
            .then(Matrix::translate(centre_x, centre_y))
    }

    /// The `cm` that maps an image's unit square onto the picture: fitted,
    /// aspect kept, inside the reader's view of the visible box shrunk by
    /// `scale`, centred, and turned so the reader sees `degrees`.
    ///
    /// The fit ignores the turn, so a turned picture can reach past the box
    /// at its corners — the same as turned text does. Shrinking it to fit
    /// would make the size jump as the angle slider moves.
    fn image_placement(&self, aspect: f64, scale: f64, degrees: f64) -> Matrix {
        let (reader_width, reader_height) = self.reader_size();
        let width = (scale * reader_width).min(scale * reader_height * aspect);
        let height = width / aspect;
        let (centre_x, centre_y) = self.centre();

        Matrix::new(width, 0.0, 0.0, height, -width / 2.0, -height / 2.0)
            .then(self.turn(degrees))
            .then(Matrix::translate(centre_x, centre_y))
    }
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
    use crate::fixture::{document_with_content, helvetica_resources, solid_png};
    use crate::parse::read_page_object_content;
    use lopdf::{dictionary, Dictionary, Object};
    use pdf_document::{ImageItem, PageId, Rect, TextRun};

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

    fn under(watermark: Watermark) -> Watermark {
        Watermark {
            layer: WatermarkLayer::UnderContent,
            ..watermark
        }
    }

    /// A picture `width` × `height` pixels, upright, covering the whole
    /// page it fits in, on top of the content.
    fn picture(width: u32, height: u32) -> Watermark {
        Watermark {
            mark: WatermarkMark::Image {
                bytes: solid_png(width, height, false),
                scale: 1.0,
            },
            layer: WatermarkLayer::OverContent,
            ..Watermark::picture(vec![PageId(0)], Vec::new())
        }
    }

    fn stamp(document: &mut Document, page: ObjectId, watermark: &Watermark) {
        stamp_watermark(document, &[page], watermark).expect("stampable");
    }

    fn runs(document: &Document, page: ObjectId) -> Vec<TextRun> {
        read_page_object_content(document, page, PageId(0))
            .expect("readable page")
            .text_runs
    }

    fn images(document: &Document, page: ObjectId) -> Vec<ImageItem> {
        read_page_object_content(document, page, PageId(0))
            .expect("readable page")
            .images
    }

    fn stamped_run(document: &Document, page: ObjectId, text: &str) -> TextRun {
        runs(document, page)
            .into_iter()
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("no run reads {text:?}"))
    }

    fn only_image(document: &Document, page: ObjectId) -> ImageItem {
        let mut found = images(document, page);
        assert_eq!(found.len(), 1, "{found:?}");
        found.remove(0)
    }

    fn center(rect: Rect) -> (f64, f64) {
        (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    fn assert_near(actual: f64, expected: f64, what: &str) {
        assert!(
            (actual - expected).abs() < 0.5,
            "{what}: {actual} vs {expected}"
        );
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

    fn content_streams(document: &Document, page: ObjectId) -> Vec<String> {
        let contents = document
            .get_dictionary(page)
            .unwrap()
            .get(b"Contents")
            .unwrap();
        let Object::Array(streams) = contents else {
            panic!("the stamp is added as a second stream, got {contents:?}");
        };
        streams
            .iter()
            .map(|entry| {
                let id = entry.as_reference().unwrap();
                let stream = document.get_object(id).unwrap().as_stream().unwrap();
                String::from_utf8_lossy(&stream.content).into_owned()
            })
            .collect()
    }

    /// A second page sharing the first one's parent and resources.
    fn add_sibling_page(document: &mut Document, page: ObjectId) -> ObjectId {
        let mut sibling = document.get_dictionary(page).unwrap().clone();
        let content = document.add_object(lopdf::Stream::new(dictionary! {}, HELLO.to_vec()));
        sibling.set("Contents", Object::Reference(content));
        document.add_object(sibling)
    }

    fn count_images(document: &Document) -> usize {
        document
            .objects
            .values()
            .filter(|object| {
                object.as_stream().is_ok_and(|stream| {
                    stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image")
                })
            })
            .count()
    }

    // --- Text ------------------------------------------------------------

    #[test]
    fn the_stamp_is_new_page_text_and_the_original_content_stays() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());

        stamp(&mut document, page, &draft());

        let texts: Vec<String> = runs(&document, page).into_iter().map(|r| r.text).collect();
        assert_eq!(texts, ["Hello", "DRAFT"]);
    }

    #[test]
    fn a_flat_stamp_is_centred_on_the_page() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());

        stamp(&mut document, page, &flat("DRAFT"));

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

        stamp(&mut document, page, &flat("X"));

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

        stamp(&mut document, page, &upright);

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

        stamp(&mut document, page, &flat("DRAFT"));

        let run = stamped_run(&document, page, "DRAFT");
        assert!(run.bbox.height > run.bbox.width, "{:?}", run.bbox);
    }

    #[test]
    fn the_opacity_is_set_through_its_own_graphics_state() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());

        stamp(&mut document, page, &draft());

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

        stamp(&mut document, page, &draft());

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

        stamp(&mut document, page, &draft());

        let streams = content_streams(&document, page);
        let last = streams.last().unwrap();
        assert!(last.contains("/Artifact"), "{last}");
        assert!(last.contains("/Watermark"), "{last}");
    }

    #[test]
    fn text_helvetica_cannot_show_is_refused_before_the_page_is_touched() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());
        let before = document.clone();

        let error = stamp_watermark(&mut document, &[page], &flat("草稿")).unwrap_err();

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

    // --- Background ------------------------------------------------------

    #[test]
    fn a_background_is_painted_before_the_page_content() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());

        stamp(&mut document, page, &under(draft()));

        let texts: Vec<String> = runs(&document, page).into_iter().map(|r| r.text).collect();
        assert_eq!(texts, ["DRAFT", "Hello"]);
        let streams = content_streams(&document, page);
        assert!(streams[0].contains("/Watermark"), "{streams:?}");
        assert!(!streams[1].contains("/Watermark"), "{streams:?}");
    }

    /// The page's own content sets a transform it never undoes. A stamp on
    /// top has to cancel it; a background runs before it and must not — the
    /// same correction applied there would shrink the stamp by half.
    #[test]
    fn a_background_ignores_the_transform_the_page_leaves_behind() {
        let scaled = b"2 0 0 2 0 0 cm BT /F1 12 Tf 10 10 Td (Hello) Tj ET";

        for layer in [WatermarkLayer::OverContent, WatermarkLayer::UnderContent] {
            let (mut document, page) = document_with_content(scaled, helvetica_resources());

            stamp(
                &mut document,
                page,
                &Watermark {
                    layer,
                    ..flat("DRAFT")
                },
            );

            let (x, _) = center(stamped_run(&document, page, "DRAFT").bbox);
            assert!((x - 306.0).abs() < 1.0, "{layer:?} horizontal centre {x}");
        }
    }

    #[test]
    fn a_background_keeps_a_multi_stream_page_in_order() {
        let (mut document, page) = crate::fixture::document_with_streams(
            &[
                b"BT /F1 12 Tf 72 700 Td (One) Tj ET",
                b"BT /F1 12 Tf 72 600 Td (Two) Tj ET",
            ],
            helvetica_resources(),
        );

        stamp(&mut document, page, &under(draft()));

        let texts: Vec<String> = runs(&document, page).into_iter().map(|r| r.text).collect();
        assert_eq!(texts, ["DRAFT", "One", "Two"]);
    }

    // --- Pictures --------------------------------------------------------

    #[test]
    fn a_picture_is_fitted_inside_the_page_and_centred() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());

        // 2:1 on a 612 × 792 page at full scale: the width is what binds.
        stamp(&mut document, page, &picture(200, 100));

        let image = only_image(&document, page);
        assert_near(image.bbox.width, 612.0, "width");
        assert_near(image.bbox.height, 306.0, "height");
        let (x, y) = center(image.bbox);
        assert_near(x, 306.0, "horizontal centre");
        assert_near(y, 396.0, "vertical centre");
        assert_eq!(runs(&document, page).len(), 1, "the page's text stays");
    }

    #[test]
    fn the_scale_shrinks_the_picture_around_the_centre() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());
        let half = Watermark {
            mark: WatermarkMark::Image {
                bytes: solid_png(100, 400, false),
                scale: 0.5,
            },
            ..picture(1, 1)
        };

        // 1:4 — the height binds: half of 792.
        stamp(&mut document, page, &half);

        let image = only_image(&document, page);
        assert_near(image.bbox.height, 396.0, "height");
        assert_near(image.bbox.width, 99.0, "width");
        assert_near(center(image.bbox).0, 306.0, "horizontal centre");
    }

    /// On a page shown a quarter turn round, the reader's box is 792 wide,
    /// so a wide picture fits *that* — and is written standing up in the
    /// page's own space so it reads upright.
    #[test]
    fn a_picture_on_a_rotated_page_fits_the_page_the_reader_sees() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());
        document.get_dictionary_mut(page).unwrap().set("Rotate", 90);

        stamp(&mut document, page, &picture(200, 100));

        let image = only_image(&document, page);
        assert_near(image.bbox.height, 792.0, "page-space height (reader width)");
        assert_near(image.bbox.width, 396.0, "page-space width (reader height)");
    }

    #[test]
    fn a_picture_as_background_is_painted_first() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());

        stamp(&mut document, page, &under(picture(10, 10)));

        let streams = content_streams(&document, page);
        assert!(streams[0].contains(" Do"), "{streams:?}");
        assert!(streams[0].contains("/Watermark"), "{streams:?}");
        assert_eq!(
            only_image(&document, page).resource_xobject_name(),
            Some("ImVitela1")
        );
    }

    #[test]
    fn a_transparent_picture_keeps_its_alpha_as_a_soft_mask() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());
        let translucent = Watermark {
            mark: WatermarkMark::Image {
                bytes: solid_png(4, 4, true),
                scale: 1.0,
            },
            ..picture(1, 1)
        };

        stamp(&mut document, page, &translucent);

        let xobjects = resource_category(&document, page, b"XObject");
        let image_id = xobjects.get(b"ImVitela1").unwrap().as_reference().unwrap();
        let image = document.get_object(image_id).unwrap().as_stream().unwrap();
        assert!(image.dict.get(b"SMask").is_ok(), "{:?}", image.dict);
    }

    #[test]
    fn one_picture_on_many_pages_is_one_image_object() {
        let (mut document, first) = document_with_content(HELLO, helvetica_resources());
        let second = add_sibling_page(&mut document, first);

        stamp_watermark(&mut document, &[first, second], &picture(10, 10)).expect("stampable");

        assert_eq!(count_images(&document), 1);
        assert_eq!(images(&document, first).len(), 1);
        assert_eq!(images(&document, second).len(), 1);
    }

    #[test]
    fn bytes_that_are_not_a_picture_are_refused_before_the_page_is_touched() {
        let (mut document, page) = document_with_content(HELLO, helvetica_resources());
        let before = document.clone();
        let noise = Watermark {
            mark: WatermarkMark::Image {
                bytes: b"not a picture".to_vec(),
                scale: 1.0,
            },
            ..picture(1, 1)
        };

        let error = stamp_watermark(&mut document, &[page], &noise).unwrap_err();

        assert!(matches!(error, EditError::InvalidImage(_)), "{error:?}");
        assert_eq!(document.objects, before.objects);
        assert!(matches!(
            check_watermark(&noise),
            Err(EditError::InvalidImage(_))
        ));
        assert!(check_watermark(&picture(3, 3)).is_ok());
    }

    #[test]
    fn no_pages_writes_nothing() {
        let (mut document, _) = document_with_content(HELLO, helvetica_resources());
        let before = document.clone();

        stamp_watermark(&mut document, &[], &picture(3, 3)).expect("nothing to do");

        assert_eq!(document.objects, before.objects);
    }
}
