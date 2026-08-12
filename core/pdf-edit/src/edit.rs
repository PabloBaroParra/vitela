//! Surgical content-stream rewrites (T-154).
//!
//! Every function here targets one item and splices one byte range. What it
//! did not target comes out of the rewrite exactly as it went in — which is
//! why [`crate::parse`] carries byte spans rather than decoding to a syntax
//! tree and re-encoding it.
//!
//! Two invariants hold across all of them:
//!
//! 1. **Nothing is written until everything is checked.** A replacement the
//!    font cannot encode fails with the stream untouched (batch decision 3).
//! 2. **The item is what identifies the target, not its id.** An id is a
//!    position in a parse, and replaying several commands in one save
//!    renumbers the page as soon as one of them removes something. The
//!    target is confirmed by its own content and position, so an edit is
//!    never applied to whatever happens to occupy a stale index — and when
//!    identity cannot single one out, the edit is refused rather than aimed
//!    at a guess.
//!
//! The page is named by its **object id**, not by a `PageId`. A `PageId` is
//! a position in the page tree, and the save that replays these edits may
//! have deleted, reordered or inserted pages first — resolving positionally
//! at that point lands the edit on whichever page moved into the slot. The
//! caller that owns the page replay owns the mapping.

use crate::encoding::resolve_font;
use crate::error::EditError;
use crate::parse::matrix::Matrix;
use crate::parse::{read_located_content, LocatedContent, LocatedImage, LocatedTextRun};
use lopdf::{Dictionary, Document, Object, ObjectId};
use pdf_document::{ContentItemId, ImageItem, Rect, TextRun};

/// Replaces the text a run shows, keeping its font, size and position.
///
/// Fails with [`EditError::EncodingGap`] — leaving the document untouched —
/// when the run's font has no code for one of `after`'s characters, and with
/// [`EditError::CompositeFontNotEditable`] for Type0/CID fonts.
///
/// A `TJ` array collapses to a single string: its kerning described the old
/// glyph sequence and means nothing for the new one.
pub fn replace_text_run(
    document: &mut Document,
    page_object: ObjectId,
    item: &TextRun,
    after: &str,
) -> Result<(), EditError> {
    let located = read_located_content(document, page_object)?;
    let target = resolve_text_run(&located, item)?;

    // Encode first: an unrepresentable character must abort before any byte
    // of the document is written.
    let font = page_font(document, page_object, &target.run.resource_font_name)?;
    let codes = font.encode(after)?;

    let replacement = if target.operator == "TJ" {
        let mut array = vec![b'['];
        array.extend_from_slice(&literal_string(&codes));
        array.push(b']');
        array
    } else {
        literal_string(&codes)
    };

    let (stream_index, span) = (target.stream_index, target.operand_span.clone());
    splice(document, &located, stream_index, span, &replacement)
}

/// Deletes a run, leaving behind the advance it occupied.
///
/// The leftover is a `TJ` adjustment that paints nothing and moves the text
/// matrix exactly as far as the deleted glyphs did — without it, everything
/// after the deletion on that line would slide left, which is reflow, and
/// reflow is out of scope by decision.
pub fn remove_text_run(
    document: &mut Document,
    page_object: ObjectId,
    item: &TextRun,
) -> Result<(), EditError> {
    let located = read_located_content(document, page_object)?;
    let target = resolve_text_run(&located, item)?;

    let replacement = if target.advance_adjustment.abs() < 1e-9 {
        Vec::new()
    } else {
        format!("[{}] TJ", format_number(target.advance_adjustment)).into_bytes()
    };

    let (stream_index, span) = (target.stream_index, target.operation_span.clone());
    splice(document, &located, stream_index, span, &replacement)
}

/// Repositions an existing image so its box becomes `to`.
pub fn move_image(
    document: &mut Document,
    page_object: ObjectId,
    item: &ImageItem,
    to: Rect,
) -> Result<(), EditError> {
    place_image(document, page_object, item, to)
}

/// Rescales an existing image so its box becomes `to`.
///
/// Identical machinery to [`move_image`] — both are a new placement matrix —
/// and kept separate because the caller's intent differs, which is what the
/// undo log records.
pub fn resize_image(
    document: &mut Document,
    page_object: ObjectId,
    item: &ImageItem,
    to: Rect,
) -> Result<(), EditError> {
    place_image(document, page_object, item, to)
}

/// Deletes an image's paint operation. The XObject itself stays in
/// `/Resources`: another page, or another `Do` on this one, may still use
/// it, and pruning unreferenced resources is the writer's job, not this
/// function's.
pub fn remove_image(
    document: &mut Document,
    page_object: ObjectId,
    item: &ImageItem,
) -> Result<(), EditError> {
    let located = read_located_content(document, page_object)?;
    let target = resolve_image(&located, item)?;

    let (stream_index, span) = (target.stream_index, target.operation_span.clone());
    splice(document, &located, stream_index, span, &[])
}

/// Swaps the bytes behind an image, keeping its resource name and its place
/// on the page. The content stream is not touched at all.
///
/// **The XObject is replaced in place.** If another page references the same
/// object, it shows the new image too — v1 does not clone the resource to
/// isolate the edit.
pub fn replace_image_source(
    document: &mut Document,
    page_object: ObjectId,
    item: &ImageItem,
    bytes: &[u8],
) -> Result<(), EditError> {
    let located = read_located_content(document, page_object)?;
    resolve_image(&located, item)?;

    // Decode before locating the object to overwrite: bad bytes must not
    // leave a half-updated resource behind.
    let replacement = crate::insert::image_xobject(bytes)?;
    let object_id = image_xobject_id(document, page_object, &item.resource_xobject_name)?;

    let smask_reference = replacement
        .smask
        .map(|smask| Object::Reference(document.add_object(smask)));
    let mut image = replacement.image;
    match smask_reference {
        Some(reference) => image.dict.set("SMask", reference),
        None => {
            image.dict.remove(b"SMask");
        }
    }

    document.objects.insert(object_id, Object::Stream(image));
    Ok(())
}

/// The shared body of move and resize: leave the placement that is already
/// in the stream alone and correct it locally at the `Do`.
///
/// Rewriting the preceding `cm` instead would be shorter, and wrong whenever
/// that `cm` is composed from several transforms or shared with another
/// operator — a `q`/`Q` pair around the correction is unconditionally safe.
fn place_image(
    document: &mut Document,
    page_object: ObjectId,
    item: &ImageItem,
    to: Rect,
) -> Result<(), EditError> {
    let located = read_located_content(document, page_object)?;
    let target = resolve_image(&located, item)?;

    let inverse = target
        .ctm_at_paint
        .invert()
        .ok_or_else(|| EditError::MalformedContent {
            reason: "image is placed by a matrix that collapses to zero area".to_string(),
            offset: target.operation_span.start,
        })?;
    let correction = Matrix::placing_unit_square(to).then(inverse);

    let replacement = format!(
        "q {} {} {} {} {} {} cm /{} Do Q",
        format_number(correction.a),
        format_number(correction.b),
        format_number(correction.c),
        format_number(correction.d),
        format_number(correction.e),
        format_number(correction.f),
        item.resource_xobject_name,
    )
    .into_bytes();

    let (stream_index, span) = (target.stream_index, target.operation_span.clone());
    splice(document, &located, stream_index, span, &replacement)
}

/// Finds the run a command targets.
///
/// Identity — text, font, box — is the authority, because the id is only a
/// **position in a parse** and replaying several content commands in one
/// save renumbers the page as soon as one of them removes something.
fn resolve_text_run<'a>(
    located: &'a LocatedContent,
    item: &TextRun,
) -> Result<&'a LocatedTextRun, EditError> {
    resolve(
        &located.text_runs,
        item.id,
        |candidate| candidate.run.id,
        |candidate| same_run(&candidate.run, item),
    )
}

/// The same lookup for images, whose identity is the resource they paint and
/// where they paint it.
fn resolve_image<'a>(
    located: &'a LocatedContent,
    item: &ImageItem,
) -> Result<&'a LocatedImage, EditError> {
    resolve(
        &located.images,
        item.id,
        |candidate| candidate.item.id,
        |candidate| same_image(&candidate.item, item),
    )
}

/// Picks the one item `id` and `matches` agree on.
///
/// A single identity match is the answer, whatever its id says — that is the
/// case a batch with an earlier removal produces, and recovering from it is
/// the whole reason identity outranks the id.
///
/// Several matches mean the page paints two things this command cannot tell
/// apart: the same string twice at the same spot, or the same XObject twice
/// under the same matrix. Both are legal and both occur in real files. The
/// id breaks the tie **only when it still lands on a matching item** — which
/// is exactly the case where nothing renumbered it. Otherwise the id is
/// known-stale and choosing by document order would be a coin flip on which
/// of two identical items the user's edit lands, so this refuses.
///
/// Refusing is the conservative half of the trade: an ambiguous target
/// fails the save with [`EditError::AmbiguousItem`] instead of silently
/// rewriting the wrong one. A shell that hits it can re-read the page and
/// record the command against a fresh parse.
fn resolve<T>(
    candidates: &[T],
    id: ContentItemId,
    id_of: impl Fn(&T) -> ContentItemId,
    matches: impl Fn(&T) -> bool,
) -> Result<&T, EditError> {
    let hits: Vec<&T> = candidates
        .iter()
        .filter(|candidate| matches(candidate))
        .collect();

    match hits.as_slice() {
        [] => Err(EditError::ItemNotFound(id)),
        [only] => Ok(only),
        several => {
            several
                .iter()
                .find(|hit| id_of(hit) == id)
                .copied()
                .ok_or(EditError::AmbiguousItem {
                    id,
                    matches: several.len(),
                })
        }
    }
}

fn same_run(parsed: &TextRun, held: &TextRun) -> bool {
    parsed.text == held.text
        && parsed.resource_font_name == held.resource_font_name
        && same_rect(parsed.bbox, held.bbox)
}

fn same_image(parsed: &ImageItem, held: &ImageItem) -> bool {
    parsed.resource_xobject_name == held.resource_xobject_name && same_rect(parsed.bbox, held.bbox)
}

/// Boxes are compared with a tolerance, not exactly: an earlier edit in the
/// same batch can pass a coordinate through the stream's decimal formatting
/// and back, which moves the last bits without moving the content.
fn same_rect(left: Rect, right: Rect) -> bool {
    const TOLERANCE: f64 = 1e-6;
    (left.x - right.x).abs() < TOLERANCE
        && (left.y - right.y).abs() < TOLERANCE
        && (left.width - right.width).abs() < TOLERANCE
        && (left.height - right.height).abs() < TOLERANCE
}

/// Writes `replacement` over `span` in one of the page's content streams and
/// stores the stream back. Every other stream, and every other byte of this
/// one, is left as it was.
fn splice(
    document: &mut Document,
    located: &crate::parse::LocatedContent,
    stream_index: usize,
    span: std::ops::Range<usize>,
    replacement: &[u8],
) -> Result<(), EditError> {
    let stream = &located.streams[stream_index];
    let mut bytes = Vec::with_capacity(stream.bytes.len() + replacement.len());
    bytes.extend_from_slice(&stream.bytes[..span.start]);
    bytes.extend_from_slice(replacement);
    bytes.extend_from_slice(&stream.bytes[span.end..]);

    let object = document.get_object_mut(stream.object_id)?.as_stream_mut()?;
    let was_compressed = crate::parse::declares_filter(&object.dict);
    object.set_plain_content(bytes);
    if was_compressed {
        // Re-compressing keeps an edited page from ballooning; it also means
        // "unchanged bytes" is a statement about decoded content, which is
        // the level the round-trip tests compare at.
        object.compress().map_err(EditError::from)?;
    }

    Ok(())
}

/// Resolves one of the page's font resources.
fn page_font(
    document: &Document,
    page_object: ObjectId,
    resource_name: &str,
) -> Result<crate::encoding::FontInfo, EditError> {
    let font_dict = resource_entry(document, page_object, b"Font", resource_name)
        .and_then(|object| match object {
            Object::Dictionary(dict) => Some(dict),
            _ => None,
        })
        .ok_or_else(|| EditError::FontResourceMissing {
            resource_font_name: resource_name.to_string(),
        })?;

    resolve_font(document, &font_dict, resource_name)
}

/// The object id of an image XObject, so its stream can be swapped without
/// disturbing the name that refers to it.
fn image_xobject_id(
    document: &Document,
    page_object: ObjectId,
    resource_name: &str,
) -> Result<ObjectId, EditError> {
    let resources = resources_of(document, page_object);
    let xobjects = resources
        .get(b"XObject")
        .ok()
        .and_then(|object| dereferenced_dict(document, object))
        .ok_or_else(|| EditError::MalformedContent {
            reason: format!("page has no XObject resources to hold {resource_name}"),
            offset: 0,
        })?;

    match xobjects.get(resource_name.as_bytes()) {
        Ok(Object::Reference(id)) => Ok(*id),
        // A directly embedded stream has no id to address, so there is
        // nothing to swap without rewriting the resource dictionary.
        _ => Err(EditError::MalformedContent {
            reason: format!("{resource_name} is not an indirect object"),
            offset: 0,
        }),
    }
}

fn resource_entry(
    document: &Document,
    page_object: ObjectId,
    category: &[u8],
    name: &str,
) -> Option<Object> {
    let resources = resources_of(document, page_object);
    let category = dereferenced_dict(document, resources.get(category).ok()?)?;
    let entry = category.get(name.as_bytes()).ok()?;
    match entry {
        Object::Reference(id) => document.get_object(*id).ok().cloned(),
        direct => Some(direct.clone()),
    }
}

/// The page's own or inherited resource dictionary. Mirrors the lookup in
/// [`crate::parse`] so an edit resolves exactly the font the parse did.
fn resources_of(document: &Document, page_id: ObjectId) -> Dictionary {
    let mut current = match document.get_dictionary(page_id) {
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

/// Wraps code bytes as a PDF literal string.
///
/// The three bytes that would otherwise change the string's structure are
/// escaped, and anything outside printable ASCII is written as an octal
/// escape so the stream stays text-safe whatever the encoding produced.
pub(crate) fn literal_string(codes: &[u8]) -> Vec<u8> {
    let mut out = vec![b'('];
    for &code in codes {
        match code {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(code);
            }
            0x20..=0x7E => out.push(code),
            other => out.extend_from_slice(format!("\\{other:03o}").as_bytes()),
        }
    }
    out.push(b')');
    out
}

/// Formats a number the way a content stream wants it: no exponent, no
/// trailing zeros, and no `-0`.
///
/// Nine decimals, not the two or three a hand-written stream tends to use.
/// A placement correction is an inverted matrix, so its entries are often
/// repeating fractions, and rounding those early compounds through the
/// multiplication into a visible offset.
const DECIMALS: usize = 9;

pub(crate) fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let scale = 10f64.powi(DECIMALS as i32);
    let rounded = (value * scale).round() / scale;
    if rounded == 0.0 {
        return "0".to_string();
    }
    let mut text = format!("{rounded:.DECIMALS$}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;
    use crate::parse::{page_object_id, read_page_content};
    use lopdf::dictionary;
    use pdf_document::{ContentItemId, PageContent, PageId};

    const HELLO: &[u8] = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";

    fn stream_bytes(document: &Document) -> Vec<u8> {
        let page_dict = document
            .get_dictionary(page_of(document))
            .expect("page dictionary");
        crate::parse::page_streams(document, page_dict)
            .expect("readable streams")
            .into_iter()
            .flat_map(|stream| stream.bytes)
            .collect()
    }

    fn content_of(document: &Document) -> PageContent {
        read_page_content(document, PageId(0)).expect("readable page")
    }

    fn run(document: &Document, id: u64) -> TextRun {
        content_of(document)
            .text_run(ContentItemId(id))
            .expect("run is present")
            .clone()
    }

    fn image_of(document: &Document, id: u64) -> ImageItem {
        content_of(document)
            .image(ContentItemId(id))
            .expect("image is present")
            .clone()
    }

    /// The fixtures are single-page, so the page every test edits is the one
    /// `PageId(0)` resolves to. The editing API takes the object id, which is
    /// the point: a `PageId` stops being a position the moment a save moves
    /// pages around.
    fn page_of(document: &Document) -> ObjectId {
        page_object_id(document, PageId(0)).expect("page 0")
    }

    fn text_document(content: &[u8]) -> (Document, ObjectId) {
        fixture::document_with_content(content, fixture::helvetica_resources())
    }

    fn image_document(content: &[u8]) -> (Document, ObjectId) {
        fixture::document_with_content(content, fixture::image_resources())
    }

    // --- replace_text_run -------------------------------------------------

    #[test]
    fn replacing_a_run_reads_back_as_the_new_text() {
        let (mut document, page) = text_document(HELLO);
        let target = run(&document, 0);

        replace_text_run(&mut document, page, &target, "Goodbye").expect("encodable");

        assert_eq!(run(&document, 0).text, "Goodbye");
    }

    /// The acceptance criterion in one test: everything outside the replaced
    /// operand survives byte for byte.
    #[test]
    fn replacing_a_run_touches_only_its_operand() {
        let (mut document, page) = text_document(HELLO);
        let target = run(&document, 0);

        replace_text_run(&mut document, page, &target, "Bye").expect("encodable");

        assert_eq!(
            stream_bytes(&document),
            b"BT /F1 12 Tf 100 700 Td (Bye) Tj ET"
        );
    }

    #[test]
    fn a_replacement_keeps_the_run_font_size_and_position() {
        let (mut document, page) = text_document(HELLO);
        let target = run(&document, 0);

        replace_text_run(&mut document, page, &target, "Hey").expect("encodable");

        let edited = run(&document, 0);
        assert_eq!(edited.resource_font_name, "F1");
        assert_eq!(edited.bbox.x, target.bbox.x);
        assert_eq!(edited.bbox.y, target.bbox.y);
    }

    /// Decision 3, enforced: the gap is detected before any byte moves.
    #[test]
    fn an_unrepresentable_replacement_leaves_the_stream_exactly_as_it_was() {
        let (mut document, page) = text_document(HELLO);
        let target = run(&document, 0);
        let before = stream_bytes(&document);

        let error = replace_text_run(&mut document, page, &target, "日本語")
            .expect_err("a Latin font cannot write this");

        assert!(matches!(
            error,
            EditError::EncodingGap {
                character: '日',
                ..
            }
        ));
        assert_eq!(
            stream_bytes(&document),
            before,
            "nothing may have been written"
        );
    }

    #[test]
    fn a_composite_font_run_refuses_replacement_without_touching_the_stream() {
        let resources = dictionary! {
            "Font" => dictionary! {
                "F1" => dictionary! {
                    "Subtype" => "Type0",
                    "BaseFont" => "AAAAAA+Noto",
                },
            },
        };
        let (mut document, page) = fixture::document_with_content(HELLO, resources);
        let target = run(&document, 0);
        let before = stream_bytes(&document);

        let error =
            replace_text_run(&mut document, page, &target, "abc").expect_err("v1 refuses Type0");

        assert!(matches!(error, EditError::CompositeFontNotEditable { .. }));
        assert_eq!(stream_bytes(&document), before);
    }

    #[test]
    fn parentheses_and_backslashes_are_escaped_when_written() {
        let (mut document, page) = text_document(HELLO);
        let target = run(&document, 0);

        replace_text_run(&mut document, page, &target, r"a(b)c\d").expect("encodable");

        assert!(
            String::from_utf8_lossy(&stream_bytes(&document)).contains(r"(a\(b\)c\\d)"),
            "an unescaped delimiter would end the string early and corrupt the stream"
        );
        assert_eq!(run(&document, 0).text, r"a(b)c\d");
    }

    #[test]
    fn a_replacement_needing_high_codes_is_written_as_octal_escapes() {
        let (mut document, page) = text_document(HELLO);
        let target = run(&document, 0);

        replace_text_run(&mut document, page, &target, "café").expect("winansi covers this");

        assert_eq!(run(&document, 0).text, "café");
    }

    #[test]
    fn replacing_a_tj_array_run_collapses_it_to_one_string() {
        let (mut document, page) = text_document(b"BT /F1 12 Tf 0 700 Td [(He) -20 (llo)] TJ ET");
        let target = run(&document, 0);

        replace_text_run(&mut document, page, &target, "Bye").expect("encodable");

        assert_eq!(run(&document, 0).text, "Bye");
        assert_eq!(
            stream_bytes(&document),
            b"BT /F1 12 Tf 0 700 Td [(Bye)] TJ ET"
        );
    }

    /// Ids are positions in a parse, not stored identities. An item that no
    /// longer matches the document means the shell is holding a stale read,
    /// and applying the edit anyway would rewrite the wrong run.
    #[test]
    fn an_item_that_no_longer_matches_the_document_is_refused() {
        let (mut document, page) = text_document(HELLO);
        let mut stale = run(&document, 0);
        stale.text = "something else".to_string();

        let error = replace_text_run(&mut document, page, &stale, "Bye")
            .expect_err("stale reads must not be applied");

        assert_eq!(error, EditError::ItemNotFound(ContentItemId(0)));
    }

    /// Ids are positions in a parse, so an earlier edit in the same batch
    /// can renumber them. The run's own identity — text, font, position —
    /// does not move, so it is the authority and the id is only a fast path.
    #[test]
    fn a_renumbered_id_is_recovered_from_the_item_itself() {
        let (mut document, page) = text_document(HELLO);
        let mut renumbered = run(&document, 0);
        renumbered.id = ContentItemId(99);

        replace_text_run(&mut document, page, &renumbered, "Bye").expect("still findable");

        assert_eq!(run(&document, 0).text, "Bye");
    }

    /// The case that actually happens when a save replays several content
    /// commands: a removal shifts every later run down one.
    #[test]
    fn an_edit_still_finds_its_run_after_an_earlier_removal_renumbered_the_page() {
        let (mut document, page) =
            text_document(b"BT /F1 12 Tf 0 700 Td (aaa) Tj (bbb) Tj (ccc) Tj ET");
        let middle = run(&document, 1);
        let last = run(&document, 2);

        remove_text_run(&mut document, page, &middle).expect("removable");
        // `last` still says id 2; the page now numbers it 1.
        replace_text_run(&mut document, page, &last, "zzz").expect("still findable");

        let texts: Vec<String> = content_of(&document)
            .text_runs
            .iter()
            .map(|run| run.text.clone())
            .collect();
        assert_eq!(texts, vec!["aaa".to_string(), "zzz".to_string()]);
    }

    /// The other half of "identity is the authority": when identity matches
    /// two items, it has stopped identifying anything. A page really can
    /// paint the same string twice at the same spot — over-printing for a
    /// faux-bold effect is the usual reason — and once a removal has
    /// renumbered the page, nothing left in the command can say which of the
    /// two the user meant. Picking the first would be a coin flip.
    #[test]
    fn an_edit_that_matches_two_identical_runs_is_refused_rather_than_guessed() {
        let (mut document, page) =
            text_document(b"BT /F1 12 Tf 0 700 Td (gone) Tj 0 0 Td (dup) Tj 0 0 Td (dup) Tj ET");
        let first = run(&document, 0);
        let mut target = run(&document, 1);
        // What a batch does: the earlier removal renumbers the duplicates, so
        // the id this command still carries no longer picks one out.
        target.id = ContentItemId(9);

        remove_text_run(&mut document, page, &first).expect("removable");
        let error = replace_text_run(&mut document, page, &target, "zzz")
            .expect_err("two runs match; neither can be singled out");

        assert_eq!(
            error,
            EditError::AmbiguousItem {
                id: ContentItemId(9),
                matches: 2
            }
        );
        let texts: Vec<String> = content_of(&document)
            .text_runs
            .iter()
            .map(|run| run.text.clone())
            .collect();
        assert_eq!(
            texts,
            vec!["dup".to_string(), "dup".to_string()],
            "a refused edit must leave the page exactly as it was"
        );
    }

    /// Duplicates alone are not ambiguous: while the id still lands on one of
    /// them, nothing has renumbered it, and that is the item the command
    /// meant. Refusing here would make over-printed text uneditable.
    #[test]
    fn a_still_valid_id_picks_one_duplicate_out_of_two() {
        let (mut document, page) =
            text_document(b"BT /F1 12 Tf 0 700 Td (dup) Tj 0 0 Td (dup) Tj ET");
        let second = run(&document, 1);

        replace_text_run(&mut document, page, &second, "changed").expect("the id still resolves");

        let texts: Vec<String> = content_of(&document)
            .text_runs
            .iter()
            .map(|run| run.text.clone())
            .collect();
        assert_eq!(texts, vec!["dup".to_string(), "changed".to_string()]);
    }

    /// The same rule for images, whose identity is thinner still: the
    /// resource name and the box, nothing else.
    #[test]
    fn an_image_edit_matching_two_identical_placements_is_refused() {
        let (mut document, page) =
            image_document(b"q 10 0 0 10 0 0 cm /Im1 Do Q q 5 0 0 5 60 60 cm /Im1 Do Q q 5 0 0 5 60 60 cm /Im1 Do Q");
        let first = image_of(&document, 0);
        let mut target = image_of(&document, 2);
        target.id = ContentItemId(7);

        remove_image(&mut document, page, &first).expect("removable");
        let error = move_image(
            &mut document,
            page,
            &target,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
        )
        .expect_err("two placements match");

        assert!(matches!(error, EditError::AmbiguousItem { matches: 2, .. }));
    }

    #[test]
    fn an_image_edit_survives_an_earlier_removal_too() {
        let (mut document, page) =
            image_document(b"q 10 0 0 10 0 0 cm /Im1 Do Q q 20 0 0 20 90 90 cm /Im1 Do Q");
        let first = image_of(&document, 0);
        let second = image_of(&document, 1);

        remove_image(&mut document, page, &first).expect("removable");
        resize_image(
            &mut document,
            page,
            &second,
            Rect {
                x: 90.0,
                y: 90.0,
                width: 40.0,
                height: 40.0,
            },
        )
        .expect("still findable");

        let images = content_of(&document).images;
        assert_eq!(images.len(), 1);
        assert!((images[0].bbox.width - 40.0).abs() < 1e-6);
    }

    #[test]
    fn an_edit_survives_a_compressed_content_stream() {
        let (mut document, page) = text_document(HELLO);
        let contents_id = match document
            .get_dictionary(page)
            .expect("page dictionary")
            .get(b"Contents")
        {
            Ok(Object::Reference(id)) => *id,
            _ => panic!("the fixture uses a single content stream"),
        };
        document
            .get_object_mut(contents_id)
            .expect("content stream")
            .as_stream_mut()
            .expect("content stream")
            .compress()
            .expect("compressible");
        let target = run(&document, 0);

        replace_text_run(&mut document, page, &target, "Bye").expect("encodable");

        assert_eq!(run(&document, 0).text, "Bye");
    }

    // --- remove_text_run --------------------------------------------------

    #[test]
    fn removing_a_run_drops_it_from_the_page() {
        let (mut document, page) = text_document(HELLO);
        let target = run(&document, 0);

        remove_text_run(&mut document, page, &target).expect("removable");

        assert!(content_of(&document).text_runs.is_empty());
    }

    /// No reflow, in both directions: deleting a word must not drag the rest
    /// of the line back over the hole it left.
    #[test]
    fn removing_a_run_leaves_the_next_run_on_the_line_where_it_was() {
        let (mut document, page) = text_document(b"BT /F1 12 Tf 100 700 Td (aaa) Tj (bbb) Tj ET");
        let second_before = run(&document, 1).bbox;
        let target = run(&document, 0);

        remove_text_run(&mut document, page, &target).expect("removable");

        let remaining = content_of(&document).text_runs;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].text, "bbb");
        assert!(
            (remaining[0].bbox.x - second_before.x).abs() < 1e-6,
            "the surviving run must not move"
        );
    }

    // --- move_image / resize_image ---------------------------------------

    #[test]
    fn moving_an_image_puts_its_box_where_asked() {
        let (mut document, page) = image_document(b"q 100 0 0 50 10 20 cm /Im1 Do Q");
        let target = image_of(&document, 0);
        let destination = Rect {
            x: 300.0,
            y: 400.0,
            width: 100.0,
            height: 50.0,
        };

        move_image(&mut document, page, &target, destination).expect("movable");

        let moved = image_of(&document, 0).bbox;
        assert!((moved.x - 300.0).abs() < 1e-6 && (moved.y - 400.0).abs() < 1e-6);
        assert!((moved.width - 100.0).abs() < 1e-6 && (moved.height - 50.0).abs() < 1e-6);
    }

    #[test]
    fn resizing_an_image_reports_the_new_size() {
        let (mut document, page) = image_document(b"q 100 0 0 50 10 20 cm /Im1 Do Q");
        let target = image_of(&document, 0);
        let resized = Rect {
            x: 10.0,
            y: 20.0,
            width: 200.0,
            height: 200.0,
        };

        resize_image(&mut document, page, &target, resized).expect("resizable");

        let after = image_of(&document, 0).bbox;
        assert!((after.width - 200.0).abs() < 1e-6 && (after.height - 200.0).abs() < 1e-6);
    }

    /// The placement correction is applied inside its own `q`/`Q`, so the
    /// graphics state the rest of the page inherits is unchanged — this is
    /// the case that breaks if the preceding `cm` is rewritten instead.
    #[test]
    fn moving_an_image_leaves_another_image_sharing_its_cm_in_place() {
        let (mut document, page) = image_document(b"q 10 0 0 10 5 5 cm /Im1 Do /Im1 Do Q");
        let second_before = image_of(&document, 1).bbox;
        let target = image_of(&document, 0);
        let destination = Rect {
            x: 500.0,
            y: 500.0,
            width: 10.0,
            height: 10.0,
        };

        move_image(&mut document, page, &target, destination).expect("movable");

        let second_after = image_of(&document, 1).bbox;
        assert_eq!(
            second_before, second_after,
            "the second image shares the cm and must not have moved"
        );
    }

    #[test]
    fn an_image_placed_by_a_collapsed_matrix_cannot_be_moved() {
        let (mut document, page) = image_document(b"q 0 0 0 0 5 5 cm /Im1 Do Q");
        let target = image_of(&document, 0);

        let error = move_image(
            &mut document,
            page,
            &target,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
        )
        .expect_err("a degenerate placement has no inverse");

        assert!(matches!(error, EditError::MalformedContent { .. }));
    }

    // --- remove_image -----------------------------------------------------

    #[test]
    fn removing_an_image_drops_it_from_the_page() {
        let (mut document, page) = image_document(b"q 100 0 0 50 10 20 cm /Im1 Do Q");
        let target = image_of(&document, 0);

        remove_image(&mut document, page, &target).expect("removable");

        assert!(content_of(&document).images.is_empty());
    }

    #[test]
    fn removing_one_image_keeps_the_other() {
        let (mut document, page) =
            image_document(b"q 10 0 0 10 5 5 cm /Im1 Do Q q 20 0 0 20 50 50 cm /Im1 Do Q");
        let target = image_of(&document, 0);

        remove_image(&mut document, page, &target).expect("removable");

        let remaining = content_of(&document).images;
        assert_eq!(remaining.len(), 1);
        assert!((remaining[0].bbox.x - 50.0).abs() < 1e-6);
    }

    // --- replace_image_source --------------------------------------------

    fn png_bytes(width: u32, height: u32, alpha: bool) -> Vec<u8> {
        use image::{DynamicImage, ImageFormat, RgbImage, RgbaImage};
        use std::io::Cursor;

        let dynamic = if alpha {
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(
                width,
                height,
                image::Rgba([1, 2, 3, 4]),
            ))
        } else {
            DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, image::Rgb([1, 2, 3])))
        };
        let mut buffer = Cursor::new(Vec::new());
        dynamic
            .write_to(&mut buffer, ImageFormat::Png)
            .expect("encode png");
        buffer.into_inner()
    }

    #[test]
    fn replacing_an_image_source_updates_its_dimensions_and_keeps_the_stream() {
        let (mut document, page) = image_document(b"q 100 0 0 50 10 20 cm /Im1 Do Q");
        let target = image_of(&document, 0);
        let before = stream_bytes(&document);

        replace_image_source(&mut document, page, &target, &png_bytes(8, 4, false))
            .expect("decodable png");

        assert_eq!(
            stream_bytes(&document),
            before,
            "the source swap happens in the xobject, not the content stream"
        );
        let xobject = image_xobject(&document);
        assert_eq!(xobject.get(b"Width").expect("width"), &Object::Integer(8));
        assert_eq!(xobject.get(b"Height").expect("height"), &Object::Integer(4));
    }

    #[test]
    fn replacing_an_image_source_keeps_its_place_on_the_page() {
        let (mut document, page) = image_document(b"q 100 0 0 50 10 20 cm /Im1 Do Q");
        let target = image_of(&document, 0);

        replace_image_source(&mut document, page, &target, &png_bytes(8, 4, false))
            .expect("decodable png");

        assert_eq!(image_of(&document, 0).bbox, target.bbox);
    }

    #[test]
    fn a_replacement_image_with_alpha_gains_a_soft_mask() {
        let (mut document, page) = image_document(b"q 10 0 0 10 0 0 cm /Im1 Do Q");
        let target = image_of(&document, 0);

        replace_image_source(&mut document, page, &target, &png_bytes(2, 2, true))
            .expect("decodable png");

        assert!(image_xobject(&document).has(b"SMask"));
    }

    #[test]
    fn undecodable_replacement_bytes_are_refused() {
        let (mut document, page) = image_document(b"q 10 0 0 10 0 0 cm /Im1 Do Q");
        let target = image_of(&document, 0);

        let error = replace_image_source(&mut document, page, &target, b"not an image")
            .expect_err("garbage must not be written");

        assert!(matches!(error, EditError::InvalidImage(_)));
    }

    fn image_xobject(document: &Document) -> Dictionary {
        let resources = document
            .get_dictionary(page_of(document))
            .expect("page dictionary")
            .get(b"Resources")
            .expect("resources")
            .as_dict()
            .expect("resources dictionary")
            .clone();
        let xobjects = resources
            .get(b"XObject")
            .expect("xobjects")
            .as_dict()
            .expect("xobject dictionary")
            .clone();
        match xobjects.get(b"Im1").expect("Im1") {
            Object::Reference(id) => document
                .get_object(*id)
                .expect("xobject")
                .as_stream()
                .expect("image stream")
                .dict
                .clone(),
            Object::Stream(stream) => stream.dict.clone(),
            other => panic!("unexpected xobject form: {other:?}"),
        }
    }
}
