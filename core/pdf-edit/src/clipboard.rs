//! The content clipboard (B25 P4, T-209): copying a selection out of a page
//! as a [`ContentClip`], and pasting one onto any page.
//!
//! A clip is a **one-page PDF**, not a list of item references. Its content
//! stream holds every copied item repainted with the state it was painted
//! under — its CTM, colours, line parameters, font and size — and its
//! resources are deep copies of whatever those operators name. Nothing in it
//! points back at the document it came from, so a paste can land on another
//! page, and later on another document, without the source being open.
//!
//! Pasting is the reverse import: each resource the clip names is copied into
//! the destination file and registered on the page under a name the page does
//! not use yet (`/VClip{n}`) — reusing the clip's own `/F1` would collide with
//! whatever the destination page already calls `/F1`, the trap inserting text
//! already hit. The content is renamed to match and appended after everything
//! else on the page, shifted so the clip's box lands where the user asked.

use std::collections::HashMap;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use pdf_document::{ContentClip, ImageSource, PageContent, PageId, Rect};

use crate::edit::{format_number, resolve_image, resolve_text_run};
use crate::error::EditError;
use crate::insert::{
    add_resource, append_to_content, correction_matrix, dereferenced_dict, matrix_operator,
    owned_resources_snapshot,
};
use crate::parse::interpreter::FormStep;
use crate::parse::lexer::{tokenize, Operand, SpannedOperation};
use crate::parse::matrix::Matrix;
use crate::parse::{page_streams, read_located_content, read_page_object_content};
use crate::path::resolve_path;
use crate::selection::Selection;

/// The resource categories a clip's content can name, each with the
/// operators that name one. A shading (`sh`) is never written by a copy, but
/// a clip from elsewhere may carry one.
const CATEGORIES: [&str; 6] = [
    "Font",
    "XObject",
    "ExtGState",
    "ColorSpace",
    "Pattern",
    "Shading",
];

/// Copies every member of `selection` out of `page_object` into a clip, in
/// the order the page paints them, so overlapping items stack the same way
/// once pasted.
///
/// Members are named as [`crate::read_page_object_content`] reads them from
/// these bytes. Content inside a Form XObject is refused: its resource names
/// belong to the form, and this version copies what the page itself paints.
pub fn copy_selection(
    document: &Document,
    page_object: ObjectId,
    selection: &Selection,
) -> Result<ContentClip, EditError> {
    if selection.is_empty() {
        return Err(not_copyable("nothing is selected"));
    }
    let located = read_located_content(document, page_object)?;
    let bytes_of = |stream_index: usize, span: std::ops::Range<usize>| {
        located
            .streams
            .get(stream_index)
            .and_then(|stream| stream.bytes.get(span))
            .map(<[u8]>::to_vec)
            .ok_or_else(|| not_copyable("an item's bytes are not in its stream"))
    };
    let page_scope = |form_path: &[FormStep]| {
        if form_path.is_empty() {
            Ok(())
        } else {
            Err(not_copyable(
                "content inside a form xobject cannot be copied yet",
            ))
        }
    };

    // (stream, offset) is paint order; each piece repaints one item from a
    // clean graphics state of its own.
    let mut pieces: Vec<((usize, usize), Vec<u8>)> = Vec::new();
    let mut boxes: Vec<Rect> = Vec::new();

    for run in &selection.runs {
        let target = resolve_text_run(&located, run)?;
        page_scope(&target.form_path)?;
        let operand = bytes_of(target.stream_index, target.operand_span.clone())?;
        let show = if target.operator == "TJ" { "TJ" } else { "Tj" };
        let placement = target.placement;
        let mut piece = format!("q {} ", matrix_operator(placement.ctm)).into_bytes();
        piece.extend_from_slice(&target.paint_state);
        piece.extend_from_slice(
            format!(
                "BT /{} {} Tf {} Tm ",
                target.run.resource_font_name,
                format_number(placement.font_size),
                numbers(placement.text_matrix),
            )
            .as_bytes(),
        );
        piece.extend_from_slice(&operand);
        piece.extend_from_slice(format!(" {show} ET Q").as_bytes());
        pieces.push(((target.stream_index, target.operation_span.start), piece));
        boxes.push(target.run.bbox);
    }

    for image in &selection.images {
        let target = resolve_image(&located, image)?;
        page_scope(&target.form_path)?;
        let paint = match &target.item.source {
            ImageSource::Resource(name) => format!("/{name} Do").into_bytes(),
            ImageSource::Inline => bytes_of(target.stream_index, target.operation_span.clone())?,
        };
        let mut piece = format!("q {} ", matrix_operator(target.ctm_at_paint)).into_bytes();
        piece.extend_from_slice(&paint);
        piece.extend_from_slice(b" Q");
        pieces.push(((target.stream_index, target.operation_span.start), piece));
        boxes.push(target.item.bbox);
    }

    for path in &selection.paths {
        let target = resolve_path(&located, path)?;
        page_scope(&target.form_path)?;
        let mut piece = format!("q {} ", matrix_operator(target.ctm)).into_bytes();
        piece.extend_from_slice(&target.paint_state);
        piece.extend_from_slice(&crate::parse::path::rewrite(
            &target.segments,
            Matrix::IDENTITY,
            &target.paint_operator,
        ));
        piece.extend_from_slice(b" Q");
        pieces.push(((target.stream_index, target.operation_span.start), piece));
        boxes.push(target.item.bbox);
    }

    pieces.sort_by_key(|(order, _)| *order);
    let mut content = Vec::new();
    for (_, piece) in pieces {
        content.extend_from_slice(&piece);
        content.push(b'\n');
    }

    let bbox = union(&boxes);
    let resources = copy_named_resources(document, page_object, &content)?;
    Ok(ContentClip {
        pdf: clip_pdf(&content, resources, bbox)?,
        bbox,
    })
}

/// Writes `clip` onto `page_object`, its box's origin moved to `to`'s.
///
/// Every resource the clip names is imported under a fresh `/VClip{n}` name
/// and the content renamed to match, then appended after everything already
/// on the page, inside its own `q … Q`.
pub fn paste_clip(
    document: &mut Document,
    page_object: ObjectId,
    clip: &ContentClip,
    to: Rect,
) -> Result<(), EditError> {
    let (source, page, content) = open_clip(clip)?;
    let source_resources = clip_resources(&source, page);

    let mut renames: HashMap<(&'static str, String), String> = HashMap::new();
    let mut copied = HashMap::new();
    for category in CATEGORIES {
        let Some(entries) = source_resources
            .get(category.as_bytes())
            .ok()
            .and_then(|object| dereferenced_dict(&source, object))
        else {
            continue;
        };
        for (name, object) in entries.iter() {
            let fresh = free_name(document, page_object, category);
            let value = import_object(&source, document, object, &mut copied);
            add_resource(document, page_object, category.as_bytes(), &fresh, value)?;
            renames.insert(
                (category, String::from_utf8_lossy(name).into_owned()),
                fresh,
            );
        }
    }

    let renamed = rename_resources(&content, &renames)?;
    let shift = Matrix::translate(to.x - clip.bbox.x, to.y - clip.bbox.y);
    let mut operators = format!(
        "\nq {} {}\n",
        matrix_operator(correction_matrix(document, page_object)?),
        matrix_operator(shift),
    )
    .into_bytes();
    operators.extend_from_slice(&renamed);
    operators.extend_from_slice(b"\nQ\n");
    append_to_content(document, page_object, &operators)
}

/// What a pending paste paints, as `page` reads it once written: the clip's
/// own items with their boxes moved to where `to` puts them. Ids are the
/// clip's own positions; the overlay replaces them.
pub fn pasted_content(
    clip: &ContentClip,
    to: Rect,
    page: PageId,
) -> Result<PageContent, EditError> {
    let (source, clip_page, _) = open_clip(clip)?;
    let mut content = read_page_object_content(&source, clip_page, page)?;
    let shift = |bbox: Rect| Rect {
        x: bbox.x + to.x - clip.bbox.x,
        y: bbox.y + to.y - clip.bbox.y,
        ..bbox
    };
    for run in &mut content.text_runs {
        run.bbox = shift(run.bbox);
    }
    for image in &mut content.images {
        image.bbox = shift(image.bbox);
    }
    for path in &mut content.paths {
        path.bbox = shift(path.bbox);
    }
    Ok(content)
}

/// Loads a clip and checks it is one: a single page whose content keeps its
/// `q`/`Q` balanced, so a paste cannot pop the destination page's state.
fn open_clip(clip: &ContentClip) -> Result<(Document, ObjectId, Vec<u8>), EditError> {
    let source =
        Document::load_mem(&clip.pdf).map_err(|error| EditError::InvalidClip(error.to_string()))?;
    let page = match source
        .get_pages()
        .into_values()
        .collect::<Vec<_>>()
        .as_slice()
    {
        [only] => *only,
        _ => {
            return Err(EditError::InvalidClip(
                "a clip holds exactly one page".into(),
            ))
        }
    };
    let page_dict = source.get_dictionary(page)?.clone();
    let mut content = Vec::new();
    for stream in page_streams(&source, &page_dict)? {
        content.extend_from_slice(&stream.bytes);
        content.push(b'\n');
    }
    let mut depth: i64 = 0;
    for operation in tokenize(&content)? {
        match operation.operator.as_str() {
            "q" => depth += 1,
            "Q" => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return Err(EditError::InvalidClip(
                "the clip restores a graphics state it never saved".into(),
            ));
        }
    }
    Ok((source, page, content))
}

fn clip_resources(source: &Document, page: ObjectId) -> Dictionary {
    owned_resources_snapshot(source, page)
}

/// The resource category and name an operation names, if it names one.
fn named_resource(operation: &SpannedOperation) -> Option<(&'static str, usize, &str)> {
    let name_at = |index: usize| match operation.operands.get(index) {
        Some(Operand::Name(name)) => Some(name.as_str()),
        _ => None,
    };
    let (category, index) = match operation.operator.as_str() {
        "Tf" => ("Font", 0),
        "Do" => ("XObject", 0),
        "gs" => ("ExtGState", 0),
        "sh" => ("Shading", 0),
        "cs" | "CS" => {
            let name = name_at(0)?;
            // The device and pattern families are names of the format, not
            // resources of the page.
            if matches!(name, "DeviceGray" | "DeviceRGB" | "DeviceCMYK" | "Pattern") {
                return None;
            }
            ("ColorSpace", 0)
        }
        "scn" | "SCN" => ("Pattern", operation.operands.len().checked_sub(1)?),
        _ => return None,
    };
    Some((category, index, name_at(index)?))
}

/// Deep-copies every resource `content` names off `page_object` into a
/// standalone resource dictionary, keeping the names the content uses.
fn copy_named_resources(
    document: &Document,
    page_object: ObjectId,
    content: &[u8],
) -> Result<(Document, Dictionary), EditError> {
    let page_resources = owned_resources_snapshot(document, page_object);
    let mut clip = Document::with_version("1.7");
    let mut copied = HashMap::new();
    let mut resources = Dictionary::new();

    for operation in tokenize(content)? {
        let Some((category, _, name)) = named_resource(&operation) else {
            continue;
        };
        let mut entries = resources
            .get(category.as_bytes())
            .ok()
            .and_then(|object| object.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        if entries.has(name.as_bytes()) {
            continue;
        }
        let value = page_resources
            .get(category.as_bytes())
            .ok()
            .and_then(|object| dereferenced_dict(document, object))
            .and_then(|entries| entries.get(name.as_bytes()).ok().cloned())
            .ok_or_else(|| match category {
                "Font" => EditError::FontResourceMissing {
                    resource_font_name: name.to_string(),
                },
                _ => not_copyable("the content names a resource the page does not have"),
            })?;
        entries.set(
            name,
            import_object(document, &mut clip, &value, &mut copied),
        );
        resources.set(category, Object::Dictionary(entries));
    }
    Ok((clip, resources))
}

/// Finishes the one-page PDF a clip is: `content` as the page, `resources` as
/// its resources, and a media box around what it paints.
fn clip_pdf(
    content: &[u8],
    (mut clip, resources): (Document, Dictionary),
    bbox: Rect,
) -> Result<Vec<u8>, EditError> {
    let pages_id = clip.new_object_id();
    let content_id = clip.add_object(Stream::new(dictionary! {}, content.to_vec()));
    let media_box: Vec<Object> = [
        bbox.x - 1.0,
        bbox.y - 1.0,
        bbox.x + bbox.width + 1.0,
        bbox.y + bbox.height + 1.0,
    ]
    .into_iter()
    .map(|value| Object::Real(value as f32))
    .collect();
    let page_id = clip.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => media_box,
        "Contents" => content_id,
        "Resources" => resources,
    });
    clip.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = clip.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    clip.trailer.set("Root", catalog_id);
    clip.compress();

    let mut bytes = Vec::new();
    clip.save_to(&mut bytes)
        .map_err(|error| EditError::Lopdf(error.to_string()))?;
    Ok(bytes)
}

/// Copies `object` from `source` into `target`, following references, so the
/// copy shares nothing with the file it came from. A `/Parent` key is dropped:
/// resources never need one, and following it would drag a page tree along.
fn import_object(
    source: &Document,
    target: &mut Document,
    object: &Object,
    copied: &mut HashMap<ObjectId, ObjectId>,
) -> Object {
    match object {
        Object::Reference(id) => {
            if let Some(&existing) = copied.get(id) {
                return Object::Reference(existing);
            }
            let new_id = target.new_object_id();
            copied.insert(*id, new_id);
            let value = match source.get_object(*id) {
                Ok(resolved) => import_object(source, target, &resolved.clone(), copied),
                Err(_) => Object::Null,
            };
            target.objects.insert(new_id, value);
            Object::Reference(new_id)
        }
        Object::Dictionary(dict) => Object::Dictionary(import_dict(source, target, dict, copied)),
        Object::Stream(stream) => {
            let dict = import_dict(source, target, &stream.dict, copied);
            let mut copy = Stream::new(dict, stream.content.clone());
            copy.allows_compression = stream.allows_compression;
            Object::Stream(copy)
        }
        Object::Array(items) => Object::Array(
            items
                .iter()
                .map(|item| import_object(source, target, item, copied))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn import_dict(
    source: &Document,
    target: &mut Document,
    dict: &Dictionary,
    copied: &mut HashMap<ObjectId, ObjectId>,
) -> Dictionary {
    let mut out = Dictionary::new();
    for (key, value) in dict.iter() {
        if key.as_slice() == b"Parent" {
            continue;
        }
        out.set(key.clone(), import_object(source, target, value, copied));
    }
    out
}

/// The first `/VClip{n}` the page has no `category` resource by.
fn free_name(document: &Document, page_object: ObjectId, category: &str) -> String {
    let entries = owned_resources_snapshot(document, page_object)
        .get(category.as_bytes())
        .ok()
        .and_then(|object| dereferenced_dict(document, object))
        .unwrap_or_default();
    (1_u32..)
        .map(|suffix| format!("VClip{suffix}"))
        .find(|candidate| !entries.has(candidate.as_bytes()))
        .expect("a finite dictionary cannot hold every suffix")
}

/// `content` with every resource name it uses replaced by its new one.
fn rename_resources(
    content: &[u8],
    renames: &HashMap<(&'static str, String), String>,
) -> Result<Vec<u8>, EditError> {
    let mut out = Vec::with_capacity(content.len());
    let mut cursor = 0;
    for operation in tokenize(content)? {
        let Some((category, index, name)) = named_resource(&operation) else {
            continue;
        };
        let Some(fresh) = renames.get(&(category, name.to_string())) else {
            return Err(EditError::InvalidClip(format!(
                "the clip paints /{name} but carries no /{category} resource by that name"
            )));
        };
        let span = operation.operand_spans[index].clone();
        out.extend_from_slice(&content[cursor..span.start]);
        out.extend_from_slice(format!("/{fresh}").as_bytes());
        cursor = span.end;
    }
    out.extend_from_slice(&content[cursor..]);
    Ok(out)
}

/// `a b c d e f`, for a `Tm`.
fn numbers(matrix: Matrix) -> String {
    [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f]
        .map(format_number)
        .join(" ")
}

fn union(boxes: &[Rect]) -> Rect {
    let left = boxes.iter().map(|b| b.x).fold(f64::INFINITY, f64::min);
    let bottom = boxes.iter().map(|b| b.y).fold(f64::INFINITY, f64::min);
    let right = boxes
        .iter()
        .map(|b| b.x + b.width)
        .fold(f64::NEG_INFINITY, f64::max);
    let top = boxes
        .iter()
        .map(|b| b.y + b.height)
        .fold(f64::NEG_INFINITY, f64::max);
    Rect {
        x: left,
        y: bottom,
        width: right - left,
        height: top - bottom,
    }
}

fn not_copyable(reason: &str) -> EditError {
    EditError::ContentNotCopyable {
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::same_rect;
    use crate::fixture;

    /// A page with a red 2pt frame, a Helvetica word in it, and an image.
    fn source() -> (Document, ObjectId) {
        fixture::document_with_content(
            b"q 2 w 1 0 0 RG 100 600 200 50 re S Q \
              BT /F1 12 Tf 110 620 Td (Total) Tj ET \
              q 30 0 0 20 400 600 cm /Im1 Do Q",
            fixture::text_and_image_resources(),
        )
    }

    fn everything(document: &Document, page: ObjectId) -> Selection {
        let content = read_page_object_content(document, page, PageId(0)).expect("readable");
        Selection {
            runs: content.text_runs,
            images: content.images,
            paths: content.paths,
        }
    }

    /// A destination whose own `/F1` is a different font, the collision a
    /// paste must not fall into.
    fn destination() -> (Document, ObjectId) {
        let resources = dictionary! {
            "Font" => dictionary! {
                "F1" => dictionary! {
                    "Type" => "Font",
                    "Subtype" => "Type1",
                    "BaseFont" => "Courier",
                    "Encoding" => "WinAnsiEncoding",
                },
            },
        };
        fixture::document_with_content(b"BT /F1 10 Tf 50 50 Td (Mine) Tj ET", resources)
    }

    fn at(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 0.0,
            height: 0.0,
        }
    }

    fn copy_all() -> ContentClip {
        let (document, page) = source();
        copy_selection(&document, page, &everything(&document, page)).expect("copied")
    }

    fn clip_text(clip: &ContentClip) -> String {
        let (_, _, content) = open_clip(clip).expect("a clip");
        String::from_utf8_lossy(&content).into_owned()
    }

    #[test]
    fn a_clip_reads_back_on_its_own_with_every_item() {
        let (document, page) = source();
        let original = everything(&document, page);
        let clip = copy_all();

        // No source object is needed to read it: the clip is the whole PDF.
        let read = pasted_content(&clip, clip.bbox, PageId(0)).expect("a clip reads");
        assert_eq!(read.text_runs.len(), 1);
        assert_eq!(read.text_runs[0].text, "Total");
        assert!(same_rect(read.text_runs[0].bbox, original.runs[0].bbox));
        assert!(same_rect(read.images[0].bbox, original.images[0].bbox));
        assert!(same_rect(read.paths[0].bbox, original.paths[0].bbox));
        assert!(same_rect(
            clip.bbox,
            Rect {
                x: 100.0,
                y: 600.0,
                width: 330.0,
                height: 50.0
            }
        ));
    }

    #[test]
    fn a_paste_lands_where_asked_under_fresh_names() {
        let clip = copy_all();
        let (mut target, target_page) = destination();

        paste_clip(&mut target, target_page, &clip, at(20.0, 100.0)).expect("pasted");

        let content = read_page_object_content(&target, target_page, PageId(0)).expect("read");
        let pasted = content
            .text_runs
            .iter()
            .find(|run| run.text == "Total")
            .expect("the copied word is on the page");
        // Painted in its own Helvetica, not in the page's Courier /F1.
        assert_eq!(pasted.resource_font_name, "VClip1");
        let mine = content.text_runs.iter().find(|run| run.text == "Mine");
        assert_eq!(mine.map(|run| run.resource_font_name.as_str()), Some("F1"));

        let frame = content.paths.first().expect("the frame came along");
        assert!(same_rect(
            frame.bbox,
            Rect {
                x: 20.0,
                y: 100.0,
                width: 200.0,
                height: 50.0
            }
        ));
        assert_eq!(content.images.len(), 1);
        assert!((content.images[0].bbox.x - 320.0).abs() < 1e-6);
    }

    #[test]
    fn a_copied_path_keeps_its_line_width_and_colour() {
        let (document, page) = source();
        let selection = Selection {
            paths: everything(&document, page).paths,
            ..Selection::default()
        };
        let clip = copy_selection(&document, page, &selection).expect("copied");
        let text = clip_text(&clip);
        assert!(text.contains("2 w"), "{text}");
        assert!(text.contains("1 0 0 RG"), "{text}");
    }

    #[test]
    fn the_pending_overlay_reports_what_the_paste_writes() {
        let clip = copy_all();
        let (mut target, target_page) = destination();
        let to = at(60.0, 300.0);

        let predicted = pasted_content(&clip, to, PageId(0)).expect("predicted");
        paste_clip(&mut target, target_page, &clip, to).expect("pasted");
        let written = read_page_object_content(&target, target_page, PageId(0)).expect("read");

        let total = written.text_runs.iter().find(|run| run.text == "Total");
        assert!(same_rect(
            predicted.text_runs[0].bbox,
            total.expect("pasted").bbox
        ));
        assert!(same_rect(predicted.paths[0].bbox, written.paths[0].bbox));
        assert!(same_rect(predicted.images[0].bbox, written.images[0].bbox));
    }

    #[test]
    fn pasting_twice_registers_two_sets_of_names() {
        let clip = copy_all();
        let (mut target, target_page) = destination();

        paste_clip(&mut target, target_page, &clip, at(0.0, 0.0)).expect("first");
        paste_clip(&mut target, target_page, &clip, at(0.0, 200.0)).expect("second");

        let fonts: Vec<String> = read_page_object_content(&target, target_page, PageId(0))
            .expect("read")
            .text_runs
            .into_iter()
            .filter(|run| run.text == "Total")
            .map(|run| run.resource_font_name)
            .collect();
        assert_eq!(fonts, ["VClip1", "VClip2"]);
    }

    #[test]
    fn copying_keeps_paint_order() {
        let text = clip_text(&copy_all());
        let frame = text.find(" re").expect("frame");
        let word = text.find("(Total)").expect("word");
        let image = text.find("Do").expect("image");
        assert!(frame < word && word < image, "{text}");
    }

    #[test]
    fn nothing_selected_is_refused() {
        let (document, page) = source();
        assert!(matches!(
            copy_selection(&document, page, &Selection::default()),
            Err(EditError::ContentNotCopyable { .. })
        ));
    }

    #[test]
    fn a_clip_that_pops_more_state_than_it_saves_is_refused() {
        let (mut forged, _) =
            fixture::document_with_content(b"Q Q 0 0 1 1 re f", Dictionary::new());
        let mut bytes = Vec::new();
        forged.save_to(&mut bytes).expect("saved");
        let clip = ContentClip {
            pdf: bytes,
            bbox: at(0.0, 0.0),
        };

        let (mut target, target_page) = destination();
        assert!(matches!(
            paste_clip(&mut target, target_page, &clip, at(0.0, 0.0)),
            Err(EditError::InvalidClip(_))
        ));
    }

    #[test]
    fn bytes_that_are_not_a_pdf_are_refused() {
        let clip = ContentClip {
            pdf: b"not a pdf".to_vec(),
            bbox: at(0.0, 0.0),
        };
        let (mut target, target_page) = destination();
        assert!(matches!(
            paste_clip(&mut target, target_page, &clip, at(0.0, 0.0)),
            Err(EditError::InvalidClip(_))
        ));
    }
}
