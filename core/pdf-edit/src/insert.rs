//! Adding genuinely new page content (T-155).
//!
//! Structurally the easy half of this batch, and for one reason: there is no
//! encoding gap to fall into that the caller cannot avoid. A new text run
//! brings its own font resource — a standard font whose character set is
//! known — and a new image brings its own XObject, built here from the
//! bytes handed in. Nothing has to fit into a subsetted font someone else
//! chose.
//!
//! What lands is **page content**, not an annotation: the operators go into
//! the page's own content stream, so external tools extracting text or
//! images from the result see it the same way they see the rest of the page.

use crate::edit::{format_number, literal_string};
use crate::encoding::resolve_font;
use crate::error::EditError;
use crate::parse::matrix::Matrix;
use crate::parse::read_located_content;
use image::GenericImageView;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use pdf_document::{ImageItem, TextRun};

/// The base font a newly created text resource gets.
///
/// One of the standard 14, so no font program has to be embedded and the
/// WinAnsi character set is available in full.
const INSERTED_BASE_FONT: &str = "Helvetica";

/// An image XObject, plus the soft mask its alpha channel needs.
pub struct ImageXObject {
    pub image: Stream,
    pub smask: Option<Stream>,
}

/// Builds an image XObject from encoded image bytes (PNG or JPEG).
///
/// Shared with [`crate::edit::replace_image_source`] — inserting an image
/// and swapping one both need exactly this.
pub fn image_xobject(bytes: &[u8]) -> Result<ImageXObject, EditError> {
    let decoded = image::load_from_memory(bytes)
        .map_err(|error| EditError::InvalidImage(error.to_string()))?;
    let (width, height) = decoded.dimensions();
    let has_alpha = decoded.color().has_alpha();
    let rgba = decoded.to_rgba8().into_raw();

    let pixels = width as usize * height as usize;
    let mut rgb = Vec::with_capacity(pixels * 3);
    let mut alpha = Vec::with_capacity(pixels);
    for pixel in rgba.chunks_exact(4) {
        rgb.extend_from_slice(&pixel[0..3]);
        alpha.push(pixel[3]);
    }

    let smask = has_alpha.then(|| {
        Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => width,
                "Height" => height,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            alpha,
        )
    });

    Ok(ImageXObject {
        image: Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => width,
                "Height" => height,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            rgb,
        ),
        smask,
    })
}

/// Appends `run` to the page as real page content.
///
/// The font size comes from `run.bbox.height` — one em is a line — and the
/// baseline is placed so the box's bottom edge sits at the descender, which
/// is the same relationship [`crate::parse`] reports boxes with, so a run
/// read back after inserting reports the box it was asked for.
///
/// Creates `run.resource_font_name` as a standard font if the page has no
/// resource by that name; an existing one is reused as it is, and the text
/// is encoded against it, so this can still fail with an
/// [`EditError::EncodingGap`].
pub fn insert_text_run(
    document: &mut Document,
    page_object: ObjectId,
    run: &TextRun,
) -> Result<(), EditError> {
    let font_name = run.resource_font_name.clone();
    ensure_font_resource(document, page_object, &font_name)?;

    let font_dict = font_resource(document, page_object, &font_name).ok_or_else(|| {
        EditError::FontResourceMissing {
            resource_font_name: font_name.clone(),
        }
    })?;
    let font = resolve_font(document, &font_dict, &font_name)?;
    // Encode before writing anything, exactly as replacing does.
    let codes = font.encode(&run.text)?;

    let size = run.bbox.height;
    let baseline = run.bbox.y + 0.25 * size;
    let correction = correction_matrix(document, page_object)?;

    let operators = format!(
        "\nq {} BT /{} {} Tf 1 0 0 1 {} {} Tm {} Tj ET Q\n",
        matrix_operator(correction),
        font_name,
        format_number(size),
        format_number(run.bbox.x),
        format_number(baseline),
        String::from_utf8_lossy(&literal_string(&codes)),
    );

    append_to_content(document, page_object, operators.as_bytes())
}

/// Appends an image to the page as real page content.
///
/// `source` carries the encoded image when this brings a new picture, and is
/// `None` when the page's resources already hold it under
/// `item.resource_xobject_name` and only the paint operation is being added —
/// which is what undoing a removal does, since removing an image leaves its
/// XObject in place.
///
/// The two cases are mutually exclusive, and the difference matters: a
/// resource dictionary maps one name to one object, so registering a *new*
/// image under a name the page already uses does not add a picture, it
/// replaces the one that was there — for every `Do` on the page that names
/// it, and for every other page sharing the dictionary. So a `source` whose
/// name is taken is refused with [`EditError::ResourceNameInUse`]; painting
/// the image that is already registered is what `source: None` is for.
pub fn insert_image(
    document: &mut Document,
    page_object: ObjectId,
    item: &ImageItem,
    source: Option<&[u8]>,
) -> Result<(), EditError> {
    if let Some(bytes) = source {
        if xobject_resource(document, page_object, &item.resource_xobject_name).is_some() {
            return Err(EditError::ResourceNameInUse {
                category: "XObject".to_string(),
                name: item.resource_xobject_name.clone(),
            });
        }

        // Decode first — a bad file must not leave a resource entry behind.
        let built = image_xobject(bytes)?;

        let mut image = built.image;
        if let Some(smask) = built.smask {
            let smask_id = document.add_object(smask);
            image.dict.set("SMask", Object::Reference(smask_id));
        }
        let image_id = document.add_object(image);

        add_resource(
            document,
            page_object,
            b"XObject",
            &item.resource_xobject_name,
            Object::Reference(image_id),
        )?;
    }

    let correction = correction_matrix(document, page_object)?;
    let placement = Matrix::placing_unit_square(item.bbox).then(correction);
    let operators = format!(
        "\nq {} /{} Do Q\n",
        matrix_operator(placement),
        item.resource_xobject_name,
    );

    append_to_content(document, page_object, operators.as_bytes())
}

/// The transform that cancels whatever CTM the page's streams leave behind,
/// so appended operators work in page coordinates.
fn correction_matrix(document: &Document, page_object: ObjectId) -> Result<Matrix, EditError> {
    let end_ctm = read_located_content(document, page_object)?.end_ctm;

    end_ctm.invert().ok_or_else(|| EditError::MalformedContent {
        reason: "page content ends with a transform that collapses to zero area".to_string(),
        offset: 0,
    })
}

fn matrix_operator(matrix: Matrix) -> String {
    format!(
        "{} {} {} {} {} {} cm",
        format_number(matrix.a),
        format_number(matrix.b),
        format_number(matrix.c),
        format_number(matrix.d),
        format_number(matrix.e),
        format_number(matrix.f),
    )
}

/// Appends operators to the page's content as a **new** stream, never by
/// rewriting one that is already there.
///
/// A `/Contents` array is defined to concatenate, so a page with one stream
/// becomes a page with two and paints exactly what it painted plus the new
/// operators. Growing the existing stream instead would be one object
/// cheaper and is the obvious implementation — and it means decoding that
/// stream, appending, and writing the result back over content this function
/// has no business touching. Decoding is where that goes wrong: a stream
/// whose filter only partly decodes comes back short, and writing it back
/// would delete the part that never decoded (see
/// [`crate::parse::decoded_stream`], which cannot see a partial decode).
///
/// Inserting content is an addition. Nothing already on the page is read,
/// rewritten or re-encoded to do it.
fn append_to_content(
    document: &mut Document,
    page_object: ObjectId,
    operators: &[u8],
) -> Result<(), EditError> {
    let contents = document
        .get_dictionary(page_object)
        .ok()
        .and_then(|dict| dict.get(b"Contents").ok())
        .cloned();
    let new_id = document.add_object(Stream::new(dictionary! {}, operators.to_vec()));

    let updated = match contents {
        Some(Object::Array(mut items)) => {
            items.push(Object::Reference(new_id));
            Object::Array(items)
        }
        Some(existing @ Object::Reference(_)) => {
            Object::Array(vec![existing, Object::Reference(new_id)])
        }
        // No `/Contents`, or one that is neither a stream reference nor an
        // array: there is nothing to preserve, so the new stream becomes it.
        _ => Object::Reference(new_id),
    };

    document
        .get_dictionary_mut(page_object)?
        .set("Contents", updated);

    Ok(())
}

/// The font resource [`ensure_font_resource`] registers for a name the page
/// does not define yet.
///
/// Factored out rather than written inline so measurement and writing cannot
/// drift apart: [`crate::edit::text_run_bbox`] resolves *this* dictionary to
/// size a run the shell has composed but not inserted yet, and a run measured
/// against different metrics than it is later drawn with would report a box
/// that does not match the glyphs.
pub(crate) fn inserted_font_dictionary() -> Dictionary {
    dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => INSERTED_BASE_FONT,
        "Encoding" => "WinAnsiEncoding",
    }
}

/// Returns a page font resource backed by the standard font used for inserted
/// text, reusing a compatible one or choosing a collision-free name.
pub(crate) fn inserted_font_resource_name(document: &Document, page_object: ObjectId) -> String {
    let resources = owned_resources_snapshot(document, page_object);
    let fonts = resources
        .get(b"Font")
        .ok()
        .and_then(|object| dereferenced_dict(document, object))
        .unwrap_or_default();

    for (name, object) in fonts.iter() {
        let Some(font) = dereferenced_dict(document, object) else {
            continue;
        };
        if crate::encoding::resolved_name(document, &font, b"Subtype").as_deref() == Some("Type1")
            && crate::encoding::resolved_name(document, &font, b"BaseFont").as_deref()
                == Some(INSERTED_BASE_FONT)
            && crate::encoding::resolved_name(document, &font, b"Encoding").as_deref()
                == Some("WinAnsiEncoding")
        {
            return String::from_utf8_lossy(name).into_owned();
        }
    }

    for suffix in 1_u32.. {
        let candidate = format!("FVitela{suffix}");
        if fonts.get(candidate.as_bytes()).is_err() {
            return candidate;
        }
    }

    unreachable!("the finite font dictionary cannot occupy every u32 suffix")
}

/// Makes the page resolve a collision-free resource name to the inserted
/// standard font and returns that name.
pub(crate) fn ensure_inserted_font_resource(
    document: &mut Document,
    page_object: ObjectId,
) -> Result<String, EditError> {
    let name = inserted_font_resource_name(document, page_object);
    ensure_font_resource(document, page_object, &name)?;
    Ok(name)
}

/// Adds a standard font under `name` unless the page already has a font
/// resource by that name.
fn ensure_font_resource(
    document: &mut Document,
    page_object: ObjectId,
    name: &str,
) -> Result<(), EditError> {
    if font_resource(document, page_object, name).is_some() {
        return Ok(());
    }

    let font_id = document.add_object(inserted_font_dictionary());

    add_resource(
        document,
        page_object,
        b"Font",
        name,
        Object::Reference(font_id),
    )
}

fn font_resource(document: &Document, page_object: ObjectId, name: &str) -> Option<Dictionary> {
    let resources = owned_resources_snapshot(document, page_object);
    let fonts = dereferenced_dict(document, resources.get(b"Font").ok()?)?;
    dereferenced_dict(document, fonts.get(name.as_bytes()).ok()?)
}

/// Whether the page already resolves `name` to an image or form XObject.
///
/// Deliberately checks the *entry*, not whether it is an image: a form
/// XObject sharing the name is just as much a collision, and overwriting one
/// is just as destructive.
fn xobject_resource(document: &Document, page_object: ObjectId, name: &str) -> Option<Object> {
    let resources = owned_resources_snapshot(document, page_object);
    let xobjects = dereferenced_dict(document, resources.get(b"XObject").ok()?)?;
    xobjects.get(name.as_bytes()).ok().cloned()
}

/// Registers `value` under `/Resources /<category> /<name>` for this page.
///
/// The page is given its **own** direct resource dictionary first, copying
/// whatever it was inheriting or sharing. Writing into an inherited or
/// indirect dictionary would quietly add the resource to every other page
/// that shares it.
fn add_resource(
    document: &mut Document,
    page_object: ObjectId,
    category: &[u8],
    name: &str,
    value: Object,
) -> Result<(), EditError> {
    let mut resources = owned_resources_snapshot(document, page_object);

    let mut category_dict = resources
        .get(category)
        .ok()
        .and_then(|object| dereferenced_dict(document, object))
        .unwrap_or_default();
    category_dict.set(name, value);
    resources.set(
        String::from_utf8_lossy(category).into_owned(),
        Object::Dictionary(category_dict),
    );

    document
        .get_dictionary_mut(page_object)?
        .set("Resources", Object::Dictionary(resources));

    Ok(())
}

/// The resource dictionary this page effectively has, resolved through
/// indirection and `/Parent` inheritance, as a detached copy.
fn owned_resources_snapshot(document: &Document, page_object: ObjectId) -> Dictionary {
    let mut current = match document.get_dictionary(page_object) {
        Ok(dict) => dict.clone(),
        Err(_) => return Dictionary::new(),
    };

    for _ in 0..32 {
        if let Ok(resources) = current.get(b"Resources") {
            if let Some(dict) = dereferenced_dict(document, resources) {
                return dict;
            }
        }
        let Ok(Object::Reference(parent_id)) = current.get(b"Parent") else {
            break;
        };
        let Ok(parent) = document.get_dictionary(*parent_id) else {
            break;
        };
        current = parent.clone();
    }

    Dictionary::new()
}

fn dereferenced_dict(document: &Document, object: &Object) -> Option<Dictionary> {
    match object {
        Object::Dictionary(dict) => Some(dict.clone()),
        Object::Reference(id) => document
            .get_object(*id)
            .ok()
            .and_then(|resolved| resolved.as_dict().ok())
            .cloned(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;
    use crate::parse::read_page_content;
    use pdf_document::{ContentItemId, FontKind, PageContent, PageId, Rect};

    fn content_of(document: &Document) -> PageContent {
        read_page_content(document, PageId(0)).expect("readable page")
    }

    fn new_run(text: &str, x: f64, y: f64) -> TextRun {
        TextRun {
            id: ContentItemId(0),
            page: PageId(0),
            bbox: Rect {
                x,
                y,
                width: 0.0,
                height: 12.0,
            },
            resource_font_name: "F9".to_string(),
            font_kind: FontKind::Standard14,
            text: text.to_string(),
        }
    }

    fn new_image(name: &str, bbox: Rect) -> ImageItem {
        ImageItem {
            id: ContentItemId(0),
            page: PageId(0),
            bbox,
            resource_xobject_name: name.to_string(),
        }
    }

    #[test]
    fn inserted_font_selection_reuses_a_compatible_resource() {
        let resources = dictionary! {
            "Font" => dictionary! {
                "Existing" => inserted_font_dictionary(),
            },
        };
        let (document, page) = fixture::document_with_content(b"", resources);

        assert_eq!(inserted_font_resource_name(&document, page), "Existing");
    }

    #[test]
    fn inserted_font_selection_skips_an_incompatible_name_collision() {
        let resources = dictionary! {
            "Font" => dictionary! {
                "FVitela1" => dictionary! {
                    "Type" => "Font",
                    "Subtype" => "Type1",
                    "BaseFont" => "Courier",
                },
            },
        };
        let (document, page) = fixture::document_with_content(b"", resources);

        assert_eq!(inserted_font_resource_name(&document, page), "FVitela2");
    }

    fn png_bytes(width: u32, height: u32, alpha: bool) -> Vec<u8> {
        use image::{DynamicImage, ImageFormat, RgbImage, RgbaImage};
        use std::io::Cursor;

        let dynamic = if alpha {
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(
                width,
                height,
                image::Rgba([9, 8, 7, 6]),
            ))
        } else {
            DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, image::Rgb([9, 8, 7])))
        };
        let mut buffer = Cursor::new(Vec::new());
        dynamic
            .write_to(&mut buffer, ImageFormat::Png)
            .expect("encode png");
        buffer.into_inner()
    }

    #[test]
    fn inserted_text_reads_back_as_a_run_on_the_page() {
        let (mut document, page) =
            fixture::document_with_content(b"", fixture::helvetica_resources());

        insert_text_run(&mut document, page, &new_run("Nuevo", 72.0, 500.0)).expect("insertable");

        let content = content_of(&document);
        assert_eq!(content.text_runs.len(), 1);
        assert_eq!(content.text_runs[0].text, "Nuevo");
    }

    #[test]
    fn inserted_text_lands_where_it_was_asked_to() {
        let (mut document, page) =
            fixture::document_with_content(b"", fixture::helvetica_resources());

        insert_text_run(&mut document, page, &new_run("Hi", 72.0, 500.0)).expect("insertable");

        let bbox = content_of(&document).text_runs[0].bbox;
        assert!((bbox.x - 72.0).abs() < 1e-6);
        assert!((bbox.y - 500.0).abs() < 1e-6);
        assert!((bbox.height - 12.0).abs() < 1e-6);
    }

    #[test]
    fn inserting_text_creates_the_font_resource_when_the_name_is_new() {
        let (mut document, page) =
            fixture::document_with_content(b"", fixture::helvetica_resources());
        assert!(font_resource(&document, page, "F9").is_none());

        insert_text_run(&mut document, page, &new_run("Hi", 0.0, 0.0)).expect("insertable");

        assert!(font_resource(&document, page, "F9").is_some());
    }

    #[test]
    fn inserting_text_reuses_a_font_resource_that_already_exists() {
        let (mut document, page) =
            fixture::document_with_content(b"", fixture::helvetica_resources());
        let mut run = new_run("Hi", 0.0, 0.0);
        run.resource_font_name = "F1".to_string();

        insert_text_run(&mut document, page, &run).expect("insertable");

        assert_eq!(content_of(&document).text_runs[0].resource_font_name, "F1");
    }

    /// The inserted font is one of the standard 14, so the gap only shows up
    /// for characters genuinely outside its character set.
    #[test]
    fn inserting_unrepresentable_text_is_refused() {
        let (mut document, page) =
            fixture::document_with_content(b"", fixture::helvetica_resources());

        let error = insert_text_run(&mut document, page, &new_run("日本語", 0.0, 0.0))
            .expect_err("a standard font cannot write this");

        assert!(matches!(error, EditError::EncodingGap { .. }));
    }

    #[test]
    fn inserted_text_accepts_the_full_winansi_range() {
        let (mut document, page) =
            fixture::document_with_content(b"", fixture::helvetica_resources());

        insert_text_run(&mut document, page, &new_run("café €", 0.0, 100.0))
            .expect("winansi covers this");

        assert_eq!(content_of(&document).text_runs[0].text, "café €");
    }

    /// Inserted content must be part of the page, not an annotation stamped
    /// over it — that is the whole distinction this batch draws.
    #[test]
    fn inserted_content_goes_into_the_content_stream_not_into_annots() {
        let (mut document, page) =
            fixture::document_with_content(b"", fixture::helvetica_resources());

        insert_text_run(&mut document, page, &new_run("Hi", 0.0, 0.0)).expect("insertable");

        assert!(
            !document
                .get_dictionary(page)
                .expect("page dictionary")
                .has(b"Annots"),
            "no annotation may have been created"
        );
        assert_eq!(content_of(&document).text_runs.len(), 1);
    }

    /// A page whose content ends inside an unbalanced transform still has to
    /// receive new content in page coordinates.
    #[test]
    fn inserted_text_ignores_a_transform_the_page_left_behind() {
        let (mut document, page) =
            fixture::document_with_content(b"q 3 0 0 3 100 100 cm", fixture::helvetica_resources());

        insert_text_run(&mut document, page, &new_run("Hi", 72.0, 500.0)).expect("insertable");

        let bbox = content_of(&document).text_runs[0].bbox;
        assert!(
            (bbox.x - 72.0).abs() < 1e-6 && (bbox.y - 500.0).abs() < 1e-6,
            "the leftover 3x scale and offset must have been cancelled"
        );
    }

    #[test]
    fn inserted_images_land_where_they_were_asked_to() {
        let (mut document, page) = fixture::document_with_content(b"", Dictionary::new());
        let target = Rect {
            x: 30.0,
            y: 40.0,
            width: 120.0,
            height: 60.0,
        };

        insert_image(
            &mut document,
            page,
            &new_image("ImNew", target),
            Some(&png_bytes(4, 2, false)),
        )
        .expect("insertable");

        let images = content_of(&document).images;
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].resource_xobject_name, "ImNew");
        assert_eq!(images[0].bbox, target);
    }

    /// Undoing a removal comes back through here with no bytes: the XObject
    /// was never deleted, so only the paint operation has to return.
    #[test]
    fn an_image_can_be_painted_again_from_a_resource_that_already_exists() {
        let (mut document, page) = fixture::document_with_content(b"", fixture::image_resources());
        let target = Rect {
            x: 12.0,
            y: 34.0,
            width: 50.0,
            height: 25.0,
        };

        insert_image(&mut document, page, &new_image("Im1", target), None)
            .expect("the resource is already registered");

        let images = content_of(&document).images;
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].bbox, target);
    }

    /// A resource dictionary maps one name to one object. Registering a new
    /// image under a name the page already uses is not an addition — it is a
    /// replacement, reaching every `Do` on the page that names it. The user
    /// asked to insert a picture, not to swap the one already there.
    #[test]
    fn inserting_an_image_under_a_name_the_page_already_uses_is_refused() {
        let (mut document, page) = fixture::document_with_content(
            b"q 10 0 0 10 0 0 cm /Im1 Do Q",
            fixture::image_resources(),
        );
        let before = image_object_id(&document, page, "Im1");
        let target = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };

        let error = insert_image(
            &mut document,
            page,
            &new_image("Im1", target),
            Some(&png_bytes(4, 4, false)),
        )
        .expect_err("the name is taken");

        assert!(matches!(
            error,
            EditError::ResourceNameInUse { ref name, .. } if name == "Im1"
        ));
        assert_eq!(
            image_object_id(&document, page, "Im1"),
            before,
            "the image already on the page must still be the one Im1 names"
        );
        assert_eq!(
            content_of(&document).images.len(),
            1,
            "and no paint operation may have been appended"
        );
    }

    /// The rule is about *registering* a resource, not about painting one:
    /// `source: None` says the XObject is already there, which is exactly
    /// what undoing a removal replays.
    #[test]
    fn painting_an_existing_resource_again_is_still_allowed() {
        let (mut document, page) = fixture::document_with_content(b"", fixture::image_resources());

        insert_image(
            &mut document,
            page,
            &new_image(
                "Im1",
                Rect {
                    x: 1.0,
                    y: 2.0,
                    width: 3.0,
                    height: 4.0,
                },
            ),
            None,
        )
        .expect("no resource is being registered");

        assert_eq!(content_of(&document).images.len(), 1);
    }

    fn image_object_id(document: &Document, page: ObjectId, name: &str) -> Option<ObjectId> {
        match xobject_resource(document, page, name) {
            Some(Object::Reference(id)) => Some(id),
            _ => None,
        }
    }

    #[test]
    fn an_inserted_image_with_alpha_gets_a_soft_mask() {
        let (mut document, page) = fixture::document_with_content(b"", Dictionary::new());
        let target = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };

        insert_image(
            &mut document,
            page,
            &new_image("ImNew", target),
            Some(&png_bytes(2, 2, true)),
        )
        .expect("insertable");

        let resources = owned_resources_snapshot(&document, page);
        let xobjects = dereferenced_dict(&document, resources.get(b"XObject").expect("xobjects"))
            .expect("xobject dictionary");
        let Ok(Object::Reference(image_id)) = xobjects.get(b"ImNew") else {
            panic!("the inserted image is an indirect object");
        };
        let dict = &document
            .get_object(*image_id)
            .expect("image object")
            .as_stream()
            .expect("image stream")
            .dict;

        assert!(dict.has(b"SMask"));
    }

    #[test]
    fn undecodable_image_bytes_are_refused_before_a_resource_is_registered() {
        let (mut document, page) = fixture::document_with_content(b"", Dictionary::new());
        let target = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };

        let error = insert_image(
            &mut document,
            page,
            &new_image("ImNew", target),
            Some(b"not an image".as_slice()),
        )
        .expect_err("garbage must not be written");

        assert!(matches!(error, EditError::InvalidImage(_)));
        assert!(font_resource(&document, page, "ImNew").is_none());
    }

    /// An array `/Contents` gains a stream rather than growing its last one,
    /// which keeps every existing stream byte-identical.
    #[test]
    fn inserting_into_an_array_contents_appends_a_new_stream() {
        let (mut document, page) =
            fixture::document_with_streams(&[b"q Q", b"q Q"], fixture::helvetica_resources());

        insert_text_run(&mut document, page, &new_run("Hi", 10.0, 10.0)).expect("insertable");

        let Ok(Object::Array(items)) = document
            .get_dictionary(page)
            .expect("page dictionary")
            .get(b"Contents")
        else {
            panic!("contents stays an array");
        };
        assert_eq!(items.len(), 3);
        assert_eq!(content_of(&document).text_runs.len(), 1);
    }

    /// The invariant that protects content this function never read: an
    /// insertion adds a stream, it does not rewrite the one already there.
    #[test]
    fn inserting_leaves_the_page_existing_stream_byte_identical() {
        let (mut document, page) = fixture::document_with_content(
            b"BT /F1 12 Tf 0 700 Td (original) Tj ET",
            fixture::helvetica_resources(),
        );
        let Ok(Object::Reference(existing_id)) = document
            .get_dictionary(page)
            .expect("page dictionary")
            .get(b"Contents")
        else {
            panic!("the fixture starts with a single content stream");
        };
        let existing_id = *existing_id;
        let before = document
            .get_object(existing_id)
            .expect("content stream")
            .as_stream()
            .expect("content stream")
            .content
            .clone();

        insert_text_run(&mut document, page, &new_run("added", 10.0, 10.0)).expect("insertable");

        assert_eq!(
            document
                .get_object(existing_id)
                .expect("content stream")
                .as_stream()
                .expect("content stream")
                .content,
            before,
            "the stream that was already on the page must not have been touched"
        );
        let texts: Vec<String> = content_of(&document)
            .text_runs
            .into_iter()
            .map(|run| run.text)
            .collect();
        assert_eq!(texts, vec!["original".to_string(), "added".to_string()]);
    }

    /// The dangerous shape, end to end: a real Flate stream cut short, so it
    /// inflates to a readable *prefix* of the page. That prefix used to be
    /// treated as the whole page — appended to, and written back — deleting
    /// everything past the cut to add one line. Two things stop it now, and
    /// both matter: the parse refuses a stream that does not end, and the
    /// append would not have rewritten it anyway.
    #[test]
    fn inserting_into_a_page_with_an_undecodable_stream_does_not_destroy_it() {
        let (mut document, page) = fixture::document_with_content(
            b"BT /F1 12 Tf 0 700 Td (precious) Tj ET",
            fixture::helvetica_resources(),
        );
        let existing_id = fixture::content_stream_id(&document, page);
        let before = fixture::truncate_page_stream_to_broken_flate(&mut document, page);

        let error = insert_text_run(&mut document, page, &new_run("added", 10.0, 10.0))
            .expect_err("the page cannot be read, so it cannot be placed on");

        assert!(matches!(error, EditError::UndecodableContentStream { .. }));
        assert_eq!(
            fixture::stored_stream_bytes(&document, existing_id),
            before,
            "bytes we could not read whole are bytes we must not rewrite"
        );
    }

    #[test]
    fn a_page_with_no_contents_gains_one() {
        let (mut document, page) =
            fixture::document_with_content(b"", fixture::helvetica_resources());
        document
            .get_dictionary_mut(page)
            .expect("page dictionary")
            .remove(b"Contents");

        insert_text_run(&mut document, page, &new_run("Hi", 10.0, 10.0)).expect("insertable");

        assert_eq!(content_of(&document).text_runs.len(), 1);
    }
}
