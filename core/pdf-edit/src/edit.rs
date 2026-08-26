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

/// The box `run` would occupy if it showed `text` instead of `run.text`.
///
/// Writes nothing and resolves nothing on the page beyond the run's font —
/// this exists for a shell that has queued a replacement (or composed an
/// insertion) and needs the *pending* geometry to hit-test and outline
/// against, long before a save re-parses the page and hands back a real box.
/// Only the width changes: a replacement keeps the run's font, size and
/// origin, so nothing else about the box can move.
///
/// The width is derived one of two ways, because a [`TextRun`] on its own
/// does not carry the font size, character spacing, horizontal scale or CTM
/// that turn an advance into points:
///
/// - **The page defines the run's font** — the ordinary case, a run that was
///   parsed off the page. The known box is scaled by the ratio of the two
///   texts' advances in that font. Exact whenever the run's advance is
///   proportional to its glyph widths, which a replacement makes true going
///   forward: `replace_text_run` collapses a kerned `TJ` array to one string,
///   so the kerning that could skew the ratio is gone from the result anyway.
/// - **The page does not define it** — a run the shell composed for
///   insertion, whose resource name [`crate::insert_text_run`] has not
///   registered yet. There is no box to scale, but no unknowns either: the
///   font is the standard one insertion registers, and `insert_text_run`
///   reads `bbox.height` *as* the font size, so the advance converts to
///   points directly.
///
/// Fails with [`EditError::EncodingGap`] when either text has a character the
/// font cannot show — including `run.text` itself, which for a run parsed
/// from a font this build cannot fully decode reads back with U+FFFD. A
/// caller that only wants a better box can treat any error as "keep the one
/// you have"; nothing here is load-bearing for correctness of the edit
/// itself.
pub fn text_run_bbox(
    document: &Document,
    page_object: ObjectId,
    run: &TextRun,
    text: &str,
) -> Result<Rect, EditError> {
    let font = match page_font(document, page_object, &run.resource_font_name) {
        Ok(font) => font,
        // The composed-insertion case. Deliberately keyed on the font being
        // absent rather than on a flag the caller passes: a run naming a
        // resource the page does not have is precisely a run that has not
        // been written yet, and once it has been, the first branch takes over
        // with the real resource.
        Err(EditError::FontResourceMissing { .. }) => {
            let font = resolve_font(
                document,
                &crate::insert::inserted_font_dictionary(),
                &run.resource_font_name,
            )?;
            let width = font.width_of(&font.encode(text)?) * run.bbox.height;
            return Ok(Rect { width, ..run.bbox });
        }
        Err(error) => return Err(error),
    };

    let after = font.width_of(&font.encode(text)?);
    let before = font.width_of(&font.encode(&run.text)?);
    // A run that advances nothing gives no scale to work from — its box is
    // degenerate in the same way, so the size-from-height reading is the only
    // one left rather than a preference.
    let width = if before > 0.0 {
        run.bbox.width * after / before
    } else {
        after * run.bbox.height
    };

    Ok(Rect { width, ..run.bbox })
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

/// Repositions an existing run so its box sits at `to`'s origin, keeping
/// the font, size, colour and kerning it already had.
///
/// Only `to`'s **origin** is read. A run's width and height come from its
/// font and its text, not from a box the caller chooses — unlike an image,
/// which is painted into whatever rectangle its matrix names — so a `to`
/// that resizes is not refused, it simply has nothing to resize.
///
/// # How the rewrite keeps the rest of the page still
///
/// The run's show operation is replaced by four things, in order:
///
/// 1. `a b c d e f Tm` — the run's own text matrix, corrected so the glyphs
///    land `to - item.bbox` further along in **page** space. The correction
///    is pulled back through the CTM, so the run moves the distance the user
///    asked for even inside a scaled or rotated `cm`.
/// 2. The original operand bytes, unchanged, shown with `Tj`/`TJ`. Not
///    re-encoded and not re-kerned: whatever the file said is what still
///    paints, which is what keeps the font, the size, the fill colour and a
///    `TJ` array's kerning exactly as they were.
/// 3. `Tm` again, restoring the line matrix the operation started from — so
///    a following `Td`, `T*` or `'` measures from where it always did.
/// 4. A `TJ` adjustment reproducing the horizontal advance the text state
///    had accumulated on that line, this run's own included — so a following
///    `Tj` on the same line still starts where it always did.
///
/// Steps 3 and 4 are what make this a *move of one run* rather than a shift
/// of everything after it. Without them the page would reflow, and reflow is
/// out of scope by the same decision that shaped [`remove_text_run`].
///
/// Fails with [`EditError::TextRunNotMovable`] for a run painted by `"`
/// (see that variant's own doc), and with [`EditError::MalformedContent`]
/// when a matrix in the run's own placement collapses to zero area and
/// cannot be inverted.
pub fn move_text_run(
    document: &mut Document,
    page_object: ObjectId,
    item: &TextRun,
    to: Rect,
) -> Result<(), EditError> {
    let located = read_located_content(document, page_object)?;
    let target = resolve_text_run(&located, item)?;

    // `"` sets word and character spacing for everything after it; the
    // rewrite below emits a plain show operator and would drop both.
    if target.operator == "\"" {
        return Err(EditError::TextRunNotMovable {
            operator: target.operator.clone(),
        });
    }

    let placement = target.placement;
    let collapsed = |what: &str| EditError::MalformedContent {
        reason: format!("text is placed by a {what} that collapses to zero area"),
        offset: target.operation_span.start,
    };

    // The page-space displacement, pulled back into text space: the moved
    // matrix is the original one with a page-space translation composed on
    // its page-facing side, `Tm × CTM × T × CTM⁻¹`.
    let inverse_ctm = placement.ctm.invert().ok_or_else(|| collapsed("matrix"))?;
    let shift = Matrix::translate(to.x - item.bbox.x, to.y - item.bbox.y);
    let moved = placement
        .text_matrix
        .then(placement.ctm)
        .then(shift)
        .then(inverse_ctm);

    // How far along the line the text matrix already stood before this run
    // painted. `Tm` can only ever be `translate(carried, 0) × Tlm` — every
    // operator that sets `Tlm` sets `Tm` equal to it, and only showing text
    // moves them apart, always by a horizontal advance — so this recovers
    // that one number exactly.
    let inverse_line = placement
        .line_matrix
        .invert()
        .ok_or_else(|| collapsed("line matrix"))?;
    let carried = placement.text_matrix.then(inverse_line).e;

    let restored_advance = carried + placement.advance;
    let adjustment = if placement.advance_scale.abs() < 1e-9 {
        // No `TJ` number can displace anything at this scale. Harmless when
        // there is no displacement to reproduce, and unreachable otherwise:
        // a degenerate scale is exactly what makes the advance zero too.
        0.0
    } else {
        -restored_advance * 1000.0 / placement.advance_scale
    };

    let operand = &located.streams[target.stream_index].bytes[target.operand_span.clone()];
    let show = if target.operator == "TJ" { "TJ" } else { "Tj" };

    let mut replacement = matrix_operator(moved);
    replacement.extend_from_slice(operand);
    replacement.extend_from_slice(format!(" {show} ").as_bytes());
    replacement.extend_from_slice(&matrix_operator(placement.line_matrix));
    if adjustment.abs() >= 1e-9 {
        replacement.extend_from_slice(format!("[{}] TJ", format_number(adjustment)).as_bytes());
    }

    let (stream_index, span) = (target.stream_index, target.operation_span.clone());
    splice(document, &located, stream_index, span, &replacement)
}

/// `a b c d e f Tm `, trailing space included so operands never run together.
fn matrix_operator(matrix: Matrix) -> Vec<u8> {
    format!(
        "{} {} {} {} {} {} Tm ",
        format_number(matrix.a),
        format_number(matrix.b),
        format_number(matrix.c),
        format_number(matrix.d),
        format_number(matrix.e),
        format_number(matrix.f),
    )
    .into_bytes()
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

/// Reads back the bytes behind an existing image, re-encoded as PNG when the
/// stream is not already a standalone encoded file — so a shell can carry
/// them as `Command::ReplaceImageSource`'s `before`, which is the only way
/// undo can restore the image [`replace_image_source`] just overwrote.
///
/// Supports exactly what this crate's own writers ([`crate::insert::image_xobject`]
/// and `replace_image_source` itself) ever produce — 8-bit `DeviceGray`/
/// `DeviceRGB` samples (optionally `FlateDecode`/`LZWDecode`/`ASCII85Decode`d)
/// with an optional 8-bit `DeviceGray` `/SMask` for alpha — plus `DCTDecode`
/// (JPEG) streams, which need no re-encoding at all since `image` decodes
/// JPEG directly. Anything else (`Indexed`, `DeviceCMYK`, 16-bit samples,
/// `CCITTFaxDecode`, `JBIG2Decode`, `JPXDecode`, a mismatched `/SMask`, a
/// `/Decode` or `/Mask` entry, an `/SMask` on a `DCTDecode` stream, ...) is
/// refused with [`EditError::ImageSourceNotRecoverable`] rather than guessed
/// at — the same posture [`EditError::EncodingGap`] takes for text this
/// crate cannot re-represent.
///
/// The test for "supported" is not "can these bytes be read" but **"does
/// undo restore the same image"**: the bytes returned here come back through
/// [`replace_image_source`], so anything the round trip would drop on the
/// floor — the alpha of a JPEG that keeps it in a separate `/SMask`, a
/// `/Decode` array that remaps every sample, a `/Mask`'s transparency — has
/// to be refused here, not silently returned as a `before` that restores a
/// visibly different image.
pub fn image_source_bytes(
    document: &Document,
    page_object: ObjectId,
    item: &ImageItem,
) -> Result<Vec<u8>, EditError> {
    let located = read_located_content(document, page_object)?;
    resolve_image(&located, item)?;

    let object_id = image_xobject_id(document, page_object, &item.resource_xobject_name)?;
    let stream = document.get_object(object_id)?.as_stream()?;
    decode_image_source(document, stream, &item.resource_xobject_name)
}

fn unrecoverable(resource_xobject_name: &str) -> EditError {
    EditError::ImageSourceNotRecoverable {
        resource_xobject_name: resource_xobject_name.to_string(),
    }
}

/// A filter [`lopdf::Stream::decompressed_content`] can actually invert —
/// the set this crate's own compressed writes ever use, not every filter the
/// PDF spec defines.
fn is_stream_filter(filter: &[u8]) -> bool {
    matches!(filter, b"FlateDecode" | b"LZWDecode" | b"ASCII85Decode")
}

/// Sample semantics that live in the dictionary rather than in the samples,
/// and that a re-encode reading only the samples therefore loses: `/Decode`
/// remaps every component on the way out (`[1 0 1 0 1 0]` paints a
/// `DeviceRGB` image inverted), and `/Mask` carries stencil or colour-key
/// transparency. Neither survives the trip back through
/// [`replace_image_source`], and neither is anything this crate's own
/// writers emit, so an image carrying one is refused instead of read back
/// into a `before` that undo would restore looking different.
fn has_unreadable_sample_semantics(stream: &lopdf::Stream) -> bool {
    stream.dict.has(b"Decode") || stream.dict.has(b"Mask")
}

fn decode_image_source(
    document: &Document,
    stream: &lopdf::Stream,
    resource_xobject_name: &str,
) -> Result<Vec<u8>, EditError> {
    match stream.filters().ok().as_deref() {
        // Already an encoded JPEG file byte for byte — `image` decodes it
        // directly, so there is nothing to reconstruct.
        Some([b"DCTDecode"]) => {
            // ...but only when the file's own bytes are the whole image. A
            // JPEG has no alpha channel of its own, so an `/SMask` beside it
            // is transparency that returning the JPEG alone would drop:
            // `replace_image_source` rebuilds `/SMask` from the replacement
            // file's channels and removes it when there are none, so undoing
            // with these bytes would restore the image opaque.
            if stream.dict.has(b"SMask") || has_unreadable_sample_semantics(stream) {
                return Err(unrecoverable(resource_xobject_name));
            }
            image::load_from_memory(&stream.content)
                .map_err(|_| unrecoverable(resource_xobject_name))?;
            Ok(stream.content.clone())
        }
        Some(filters) if filters.iter().all(|filter| is_stream_filter(filter)) => {
            encode_raw_samples(document, stream, resource_xobject_name)
        }
        None => encode_raw_samples(document, stream, resource_xobject_name),
        _ => Err(unrecoverable(resource_xobject_name)),
    }
}

/// A stream's declared `/ColorSpace`, resolved through one level of
/// indirection — the shape every image this crate writes uses, and the
/// common shape everything else uses too.
fn resolved_name(document: &Document, object: &Object) -> Option<Vec<u8>> {
    match object {
        Object::Name(name) => Some(name.clone()),
        Object::Reference(id) => document
            .get_object(*id)
            .ok()
            .and_then(|resolved| resolved.as_name().ok())
            .map(<[u8]>::to_vec),
        _ => None,
    }
}

/// Reads an 8-bit `DeviceGray` or `DeviceRGB` image stream's already
/// filter-decodable samples and its optional `/SMask`, then re-encodes both
/// together as one PNG (with an alpha channel when a mask was present).
fn encode_raw_samples(
    document: &Document,
    stream: &lopdf::Stream,
    resource_xobject_name: &str,
) -> Result<Vec<u8>, EditError> {
    let refuse = || unrecoverable(resource_xobject_name);

    // Both planes come back already proven to hold exactly
    // `width * height * components` samples, so the zips below cannot
    // silently pair a short mask against a full image.
    let (width, height, components, samples) = decode_plane(document, stream, refuse)?;

    let alpha = match stream.dict.get(b"SMask") {
        Ok(Object::Reference(id)) => {
            let smask = document
                .get_object(*id)
                .map_err(|_| refuse())?
                .as_stream()
                .map_err(|_| refuse())?;
            let (mask_width, mask_height, mask_components, mask_samples) =
                decode_plane(document, smask, refuse)?;
            if mask_width != width || mask_height != height || mask_components != 1 {
                return Err(refuse());
            }
            Some(mask_samples)
        }
        Ok(_) => return Err(refuse()),
        Err(_) => None,
    };

    let dynamic = match (components, alpha) {
        (1, None) => image::DynamicImage::ImageLuma8(
            image::GrayImage::from_raw(width, height, samples).ok_or_else(refuse)?,
        ),
        (1, Some(mask)) => {
            let rgba: Vec<u8> = samples
                .iter()
                .zip(mask.iter())
                .flat_map(|(gray, a)| [*gray, *gray, *gray, *a])
                .collect();
            image::DynamicImage::ImageRgba8(
                image::RgbaImage::from_raw(width, height, rgba).ok_or_else(refuse)?,
            )
        }
        (3, None) => image::DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(width, height, samples).ok_or_else(refuse)?,
        ),
        (3, Some(mask)) => {
            let rgba: Vec<u8> = samples
                .as_chunks::<3>()
                .0
                .iter()
                .zip(mask.iter())
                .flat_map(|(rgb, a)| [rgb[0], rgb[1], rgb[2], *a])
                .collect();
            image::DynamicImage::ImageRgba8(
                image::RgbaImage::from_raw(width, height, rgba).ok_or_else(refuse)?,
            )
        }
        _ => return Err(refuse()),
    };

    let mut bytes = Vec::new();
    dynamic
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .map_err(|_| refuse())?;
    Ok(bytes)
}

/// A `/Width` or `/Height` that can actually be used as one: present,
/// positive, and inside `u32`. A negative or absurd value would otherwise
/// wrap on the cast into a plausible-looking size and then multiply out into
/// a sample count that overflows.
fn dimension(dict: &Dictionary, key: &[u8]) -> Option<u32> {
    let value = dict.get(key).and_then(Object::as_i64).ok()?;
    u32::try_from(value).ok().filter(|size| *size > 0)
}

/// One image plane's dimensions, component count (1 for `DeviceGray`, 3 for
/// `DeviceRGB`) and decoded 8-bit samples — the common read both
/// [`encode_raw_samples`] and its `/SMask` lookup need.
///
/// The samples are proven to be exactly the `width * height * components`
/// the dictionary promised before they are returned, so a caller may treat
/// the length as a fact rather than re-deriving it.
fn decode_plane(
    document: &Document,
    stream: &lopdf::Stream,
    refuse: impl Fn() -> EditError,
) -> Result<(u32, u32, usize, Vec<u8>), EditError> {
    if has_unreadable_sample_semantics(stream) {
        return Err(refuse());
    }
    let width = dimension(&stream.dict, b"Width").ok_or_else(&refuse)?;
    let height = dimension(&stream.dict, b"Height").ok_or_else(&refuse)?;
    let bpc = stream
        .dict
        .get(b"BitsPerComponent")
        .and_then(Object::as_i64)
        .map_err(|_| refuse())?;
    if bpc != 8 {
        return Err(refuse());
    }
    let color_space = stream
        .dict
        .get(b"ColorSpace")
        .ok()
        .and_then(|object| resolved_name(document, object));
    let components = match color_space.as_deref() {
        Some(b"DeviceGray") => 1,
        Some(b"DeviceRGB") => 3,
        _ => return Err(refuse()),
    };
    let expected_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(components))
        .ok_or_else(&refuse)?;

    // Decode against a ceiling the dictionary's own `/Width`x`/Height` sets,
    // rather than `get_plain_content`'s deliberately unbounded read: a
    // stream that inflates far past the size it declares is damaged or a
    // decompression bomb either way, and this crate already refuses page
    // content on the same grounds (`parse::filter`'s
    // `MAX_PAGE_CONTENT_BYTES`). The allowance over `expected_len` is one
    // byte per row, which is exactly what a PNG predictor's per-row filter
    // tag costs before `lopdf` strips it back off.
    let limit = expected_len.saturating_add(height as usize);
    let samples = stream
        .get_plain_content_with_limit(limit)
        .map_err(|_| refuse())?;
    if samples.len() != expected_len {
        return Err(refuse());
    }
    Ok((width, height, components, samples))
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

    // Whether to re-compress comes from the decode, not from a second look at
    // the dictionary: the decode is what resolved `/Filter` through
    // indirection, so it is the reading that knows.
    let content = if stream.filtered {
        Some(crate::parse::encode_flate(&bytes)?)
    } else {
        None
    };

    let object = document.get_object_mut(stream.object_id)?.as_stream_mut()?;
    match content {
        // `set_plain_content` first: it clears `/Filter` and `/DecodeParms`,
        // so the dictionary cannot end up describing an encoding the bytes no
        // longer have.
        Some(compressed) => {
            object.set_plain_content(compressed);
            object.dict.set("Filter", "FlateDecode");
        }
        None => object.set_plain_content(bytes),
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
    use lopdf::{dictionary, Stream};
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

    /// The splice path's version of the same guarantee: a stream that only
    /// half-inflates is never read, so it is never written back either. The
    /// item is stale by construction — the point is that the refusal comes
    /// from the stream, and the bytes on disk survive it untouched.
    #[test]
    fn a_truncated_content_stream_is_refused_and_left_exactly_as_it_was() {
        let (mut document, page) = text_document(HELLO);
        let target = run(&document, 0);
        let contents_id = fixture::content_stream_id(&document, page);
        let before = fixture::truncate_page_stream_to_broken_flate(&mut document, page);

        let error = replace_text_run(&mut document, page, &target, "Bye")
            .expect_err("a stream that does not end is not content");

        assert_eq!(
            error,
            EditError::UndecodableContentStream {
                object_id: contents_id
            }
        );
        assert_eq!(
            fixture::stored_stream_bytes(&document, contents_id),
            before,
            "bytes we could not read whole are bytes we must not rewrite"
        );
    }

    /// A page encoded in a way this version does not edit is refused with a
    /// different error than a damaged one, because it means something
    /// different: the file is fine, the support is not there.
    #[test]
    fn a_page_behind_an_unsupported_filter_is_refused_distinctly() {
        let (mut document, page) = text_document(HELLO);
        let target = run(&document, 0);
        let Ok(Object::Reference(contents_id)) = document
            .get_dictionary(page)
            .expect("page dictionary")
            .get(b"Contents")
        else {
            panic!("the fixture uses a single content stream");
        };
        let contents_id = *contents_id;
        document
            .get_object_mut(contents_id)
            .expect("content stream")
            .as_stream_mut()
            .expect("content stream")
            .dict
            .set("Filter", "LZWDecode");

        let error = replace_text_run(&mut document, page, &target, "Bye")
            .expect_err("LZW is not decoded by this version");

        assert!(matches!(
            error,
            EditError::UnsupportedContentStreamFilter { .. }
        ));
    }

    /// A compressed page must come back out compressed, and readable.
    ///
    /// The stream is built compressed here rather than through
    /// `Stream::compress`, which declines on a short payload — a version of
    /// this test that quietly stopped compressing would still pass while
    /// testing nothing. `PageStream::filtered` is what carries the answer
    /// from the decode to the write, so this is the test that fails if that
    /// wiring is dropped.
    #[test]
    fn editing_a_compressed_page_leaves_it_compressed_and_readable() {
        let long_line = "BT /F1 12 Tf 0 700 Td (compressible compressible compressible) Tj \
                         0 -14 Td (target) Tj ET";
        let (mut document, page) = text_document(long_line.as_bytes());
        let contents_id = fixture::content_stream_id(&document, page);
        fixture::compress_page_stream(&mut document, page);
        let target = content_of(&document)
            .text_run(ContentItemId(1))
            .expect("second run")
            .clone();

        replace_text_run(&mut document, page, &target, "edited").expect("encodable");

        let stream = document
            .get_object(contents_id)
            .expect("content stream")
            .as_stream()
            .expect("content stream");
        assert!(
            stream.dict.has(b"Filter"),
            "a page that arrived compressed must not be written back as plain bytes"
        );
        let texts: Vec<String> = content_of(&document)
            .text_runs
            .into_iter()
            .map(|run| run.text)
            .collect();
        assert_eq!(texts[1], "edited");
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

    // --- text_run_bbox ----------------------------------------------------

    /// The contract in one test: what this predicts for a replacement is what
    /// the page actually reports once the replacement is written and re-parsed.
    #[test]
    fn the_predicted_box_matches_the_one_the_page_reports_after_the_replacement() {
        let (mut document, page) = text_document(HELLO);
        let target = run(&document, 0);

        let predicted = text_run_bbox(&document, page, &target, "Goodbye").expect("encodable");
        replace_text_run(&mut document, page, &target, "Goodbye").expect("encodable");

        let actual = run(&document, 0).bbox;
        assert!(
            (predicted.width - actual.width).abs() < 1e-6,
            "predicted {} but the page reports {}",
            predicted.width,
            actual.width
        );
        assert_eq!(
            predicted.x, actual.x,
            "a replacement never moves the origin"
        );
        assert_eq!(predicted.y, actual.y);
        assert_eq!(predicted.height, actual.height, "nor changes the size");
    }

    #[test]
    fn a_longer_replacement_widens_the_box_and_a_shorter_one_narrows_it() {
        let (document, page) = text_document(HELLO);
        let target = run(&document, 0);

        let longer = text_run_bbox(&document, page, &target, "Hello there").expect("encodable");
        let shorter = text_run_bbox(&document, page, &target, "Hi").expect("encodable");

        assert!(longer.width > target.bbox.width);
        assert!(shorter.width < target.bbox.width);
    }

    #[test]
    fn the_same_text_reports_the_box_the_run_already_has() {
        let (document, page) = text_document(HELLO);
        let target = run(&document, 0);

        let unchanged = text_run_bbox(&document, page, &target, &target.text).expect("encodable");

        assert!((unchanged.width - target.bbox.width).abs() < 1e-9);
    }

    /// A run the shell composed for insertion: its resource name is not on
    /// the page yet, so there is no box to scale — the standard font
    /// insertion registers plus `bbox.height` as the size answers it exactly.
    /// "Hi" is H722 + i222 = 944/1000 em, so at 14pt the advance is 13.216pt.
    #[test]
    fn a_run_whose_font_the_page_does_not_define_is_measured_at_its_box_height() {
        let (document, page) = text_document(HELLO);
        let composed = TextRun {
            id: ContentItemId(0),
            page: PageId(0),
            bbox: Rect {
                x: 72.0,
                y: 100.0,
                width: 150.0,
                height: 14.0,
            },
            resource_font_name: "FIns1".to_string(),
            font_kind: pdf_document::FontKind::Standard14,
            text: String::new(),
        };

        let measured = text_run_bbox(&document, page, &composed, "Hi").expect("encodable");

        assert!(
            (measured.width - 13.216).abs() < 1e-3,
            "expected the Helvetica AFM advance at 14pt, got {}",
            measured.width
        );
    }

    /// The same composed run, once actually inserted, reports a box the
    /// prediction already matched — measurement and writing share the font
    /// resource for exactly this reason.
    #[test]
    fn the_predicted_box_for_a_composed_run_matches_the_inserted_one() {
        let (mut document, page) = text_document(HELLO);
        let mut composed = TextRun {
            id: ContentItemId(0),
            page: PageId(0),
            bbox: Rect {
                x: 72.0,
                y: 100.0,
                width: 150.0,
                height: 14.0,
            },
            resource_font_name: "FIns1".to_string(),
            font_kind: pdf_document::FontKind::Standard14,
            text: String::new(),
        };

        let predicted = text_run_bbox(&document, page, &composed, "Inserted").expect("encodable");
        composed.text = "Inserted".to_string();
        crate::insert_text_run(&mut document, page, &composed).expect("encodable");

        let inserted = content_of(&document)
            .text_runs
            .into_iter()
            .find(|run| run.resource_font_name == "FIns1")
            .expect("the inserted run parses back");
        assert!(
            (predicted.width - inserted.bbox.width).abs() < 1e-6,
            "predicted {} but the page reports {}",
            predicted.width,
            inserted.bbox.width
        );
    }

    #[test]
    fn a_text_the_font_cannot_show_is_refused_rather_than_measured() {
        let (document, page) = text_document(HELLO);
        let target = run(&document, 0);

        let error = text_run_bbox(&document, page, &target, "日本語")
            .expect_err("Helvetica cannot encode this");
        assert!(matches!(error, EditError::EncodingGap { .. }));
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

    // --- move_text_run ----------------------------------------------------

    /// The destination is an origin, and only an origin: a run's size comes
    /// from its font and its text, so a `to` that also resizes has nothing
    /// to resize.
    fn origin(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 0.0,
            height: 0.0,
        }
    }

    fn moved_by(before: Rect, dx: f64, dy: f64) -> Rect {
        Rect {
            x: before.x + dx,
            y: before.y + dy,
            ..before
        }
    }

    #[test]
    fn moving_a_run_puts_its_box_where_asked() {
        let (mut document, page) = text_document(HELLO);
        let target = run(&document, 0);
        let destination = moved_by(target.bbox, 40.0, -25.0);

        move_text_run(&mut document, page, &target, destination).expect("movable");

        let moved = run(&document, 0).bbox;
        assert!((moved.x - destination.x).abs() < 1e-6);
        assert!((moved.y - destination.y).abs() < 1e-6);
    }

    /// What the whole `Tm`-wrap exists for: the glyphs are the file's own
    /// bytes, so nothing about how they are drawn can drift.
    #[test]
    fn moving_a_run_keeps_its_text_font_and_size() {
        let (mut document, page) = text_document(HELLO);
        let target = run(&document, 0);

        move_text_run(&mut document, page, &target, origin(200.0, 300.0)).expect("movable");

        let moved = run(&document, 0);
        assert_eq!(moved.text, "Hello");
        assert_eq!(moved.resource_font_name, "F1");
        assert!(
            (moved.bbox.height - target.bbox.height).abs() < 1e-6
                && (moved.bbox.width - target.bbox.width).abs() < 1e-6,
            "the box is the same one, only somewhere else"
        );
    }

    /// The no-reflow invariant, the direction that actually breaks: the run
    /// after the moved one shares its line and is positioned by the advance
    /// the moved run left behind.
    #[test]
    fn moving_a_run_leaves_the_next_run_on_the_line_where_it_was() {
        let (mut document, page) = text_document(b"BT /F1 12 Tf 100 700 Td (aaa) Tj (bbb) Tj ET");
        let second_before = run(&document, 1).bbox;
        let target = run(&document, 0);

        move_text_run(&mut document, page, &target, origin(400.0, 200.0)).expect("movable");

        let after = content_of(&document);
        let second = after
            .text_runs
            .iter()
            .find(|candidate| candidate.text == "bbb")
            .expect("the second run survives");
        assert!(
            (second.bbox.x - second_before.x).abs() < 1e-6
                && (second.bbox.y - second_before.y).abs() < 1e-6,
            "the run sharing the line must not move: {:?} was {:?}",
            second.bbox,
            second_before
        );
    }

    /// The other half of no-reflow: a `Td` after the moved run is relative
    /// to the *line* matrix, so restoring `Tm` alone would leave the next
    /// line displaced by however far the run was dragged.
    #[test]
    fn moving_a_run_leaves_the_following_line_where_it_was() {
        let (mut document, page) =
            text_document(b"BT /F1 12 Tf 100 700 Td (first) Tj 0 -20 Td (second) Tj ET");
        let second_before = run(&document, 1).bbox;
        let target = run(&document, 0);

        move_text_run(&mut document, page, &target, origin(300.0, 500.0)).expect("movable");

        let after = content_of(&document);
        let second = after
            .text_runs
            .iter()
            .find(|candidate| candidate.text == "second")
            .expect("the second run survives");
        assert!(
            (second.bbox.x - second_before.x).abs() < 1e-6
                && (second.bbox.y - second_before.y).abs() < 1e-6,
            "the next line must not move: {:?} was {:?}",
            second.bbox,
            second_before
        );
    }

    /// A run mid-line starts from a text matrix already advanced past its
    /// predecessors. Restoring the line matrix without adding that carried
    /// advance back would drag everything after it to the start of the line.
    #[test]
    fn moving_the_second_run_on_a_line_restores_the_advance_it_started_from() {
        let (mut document, page) =
            text_document(b"BT /F1 12 Tf 100 700 Td (aaa) Tj (bbb) Tj (ccc) Tj ET");
        let third_before = run(&document, 2).bbox;
        let target = run(&document, 1);

        move_text_run(&mut document, page, &target, origin(400.0, 200.0)).expect("movable");

        let after = content_of(&document);
        let third = after
            .text_runs
            .iter()
            .find(|candidate| candidate.text == "ccc")
            .expect("the third run survives");
        assert!(
            (third.bbox.x - third_before.x).abs() < 1e-6,
            "the run after the moved one must not move: {:?} was {:?}",
            third.bbox,
            third_before
        );
    }

    /// A page-space displacement has to be pulled back through the CTM, or
    /// dragging 10pt to the right inside a `2 0 0 2 0 0 cm` moves the run 20.
    #[test]
    fn a_move_inside_a_scaled_ctm_still_lands_in_page_space() {
        let (mut document, page) =
            text_document(b"q 2 0 0 2 0 0 cm BT /F1 12 Tf 50 300 Td (Hello) Tj ET Q");
        let target = run(&document, 0);
        let destination = moved_by(target.bbox, 30.0, 10.0);

        move_text_run(&mut document, page, &target, destination).expect("movable");

        let moved = run(&document, 0).bbox;
        assert!(
            (moved.x - destination.x).abs() < 1e-6 && (moved.y - destination.y).abs() < 1e-6,
            "expected {destination:?}, got {moved:?}"
        );
    }

    /// `TJ` kerning is the file's own, and a move must not flatten it the
    /// way a replacement deliberately does.
    #[test]
    fn moving_a_kerned_run_keeps_its_array_operand() {
        let (mut document, page) = text_document(b"BT /F1 12 Tf 100 700 Td [(He) -20 (llo)] TJ ET");
        let target = run(&document, 0);

        move_text_run(&mut document, page, &target, origin(200.0, 400.0)).expect("movable");

        let bytes = stream_bytes(&document);
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            text.contains("[(He) -20 (llo)] TJ"),
            "the kerned array must survive verbatim: {text}"
        );
    }

    /// `'` shows text on the next line. Its line advance is already part of
    /// the matrices recorded at paint, so the rewrite reproduces it by
    /// restoring them rather than by re-emitting the operator.
    #[test]
    fn moving_a_run_shown_with_the_next_line_operator_keeps_the_following_line_put() {
        let (mut document, page) =
            text_document(b"BT /F1 10 Tf 14 TL 0 700 Td (first) ' (second) ' ET");
        let second_before = run(&document, 1).bbox;
        let target = run(&document, 0);

        move_text_run(&mut document, page, &target, origin(300.0, 500.0)).expect("movable");

        let after = content_of(&document);
        let second = after
            .text_runs
            .iter()
            .find(|candidate| candidate.text == "second")
            .expect("the second run survives");
        assert!(
            (second.bbox.x - second_before.x).abs() < 1e-6
                && (second.bbox.y - second_before.y).abs() < 1e-6,
            "the next line must not move: {:?} was {:?}",
            second.bbox,
            second_before
        );
    }

    /// `"` sets spacing for everything after it. Refused rather than
    /// rewritten into something that silently drops those two state changes.
    #[test]
    fn a_run_shown_with_the_spacing_operator_refuses_to_move() {
        let (mut document, page) = text_document(b"BT /F1 10 Tf 12 TL 0 700 Td 5 1 (spaced) \" ET");
        let target = run(&document, 0);
        let before = stream_bytes(&document);

        let error =
            move_text_run(&mut document, page, &target, origin(200.0, 200.0)).expect_err("refused");

        assert!(matches!(
            error,
            EditError::TextRunNotMovable { ref operator } if operator == "\""
        ));
        assert_eq!(
            stream_bytes(&document),
            before,
            "nothing may have been written"
        );
    }

    #[test]
    fn moving_a_run_that_is_not_on_the_page_writes_nothing() {
        let (mut document, page) = text_document(HELLO);
        let mut stranger = run(&document, 0);
        stranger.text = "Elsewhere".to_string();
        let before = stream_bytes(&document);

        let error = move_text_run(&mut document, page, &stranger, origin(200.0, 200.0))
            .expect_err("no such run");

        assert!(matches!(error, EditError::ItemNotFound(_)));
        assert_eq!(stream_bytes(&document), before);
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

    // --- image_source_bytes ------------------------------------------------

    /// Builds a one-page document with `/Im1` set to `pixels` (`DeviceRGB`,
    /// `width`x`height`, 8bpc), forcibly `FlateDecode`d regardless of size —
    /// `Stream::compress` only applies when it saves bytes, and a test-sized
    /// image rarely does, so the filter is set by hand here to exercise
    /// [`decode_image_source`]'s Flate branch rather than its no-filter one.
    fn flate_image_document(width: i64, height: i64, pixels: Vec<u8>) -> (Document, ObjectId) {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&pixels).expect("compress fixture pixels");
        let compressed = encoder.finish().expect("finish fixture compression");

        let stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => width,
                "Height" => height,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "FlateDecode",
            },
            compressed,
        );
        let resources = dictionary! { "XObject" => dictionary! { "Im1" => stream } };
        image_document_with_resources(b"q 10 0 0 10 0 0 cm /Im1 Do Q", resources)
    }

    fn image_document_with_resources(
        content: &[u8],
        resources: Dictionary,
    ) -> (Document, ObjectId) {
        fixture::document_with_content(content, resources)
    }

    fn decoded_rgb_pixels(png: &[u8]) -> Vec<u8> {
        image::load_from_memory(png)
            .expect("decode png")
            .to_rgb8()
            .into_raw()
    }

    /// An 8-bit `DeviceGray` plane, the shape a `/SMask` takes.
    fn gray_mask_stream(width: i64, height: i64, samples: Vec<u8>) -> Stream {
        Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => width,
                "Height" => height,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            samples,
        )
    }

    /// Sets `key` on `/Im1`'s own stream dictionary after the fact — the
    /// entries these tests care about (`/SMask`, `/Decode`, `/Mask`) all live
    /// there, and building them into `dictionary!` up front would mean
    /// duplicating the whole fixture per entry.
    fn set_on_image_dict(
        document: &mut Document,
        page: ObjectId,
        target: &ImageItem,
        key: &str,
        value: Object,
    ) {
        let object_id = image_xobject_id(document, page, &target.resource_xobject_name)
            .expect("Im1 is an indirect object");
        document
            .get_object_mut(object_id)
            .expect("image xobject")
            .as_stream_mut()
            .expect("image stream")
            .dict
            .set(key, value);
    }

    /// A one-page document whose `/Im1` is a 4x4 `DCTDecode` stream, plus the
    /// JPEG bytes it holds.
    fn dct_image_document() -> (Document, ObjectId, Vec<u8>) {
        use image::{ImageFormat, RgbImage};
        use std::io::Cursor;

        let mut jpeg = Cursor::new(Vec::new());
        RgbImage::from_pixel(4, 4, image::Rgb([200, 100, 50]))
            .write_to(&mut jpeg, ImageFormat::Jpeg)
            .expect("encode jpeg fixture");
        let jpeg = jpeg.into_inner();

        let stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 4,
                "Height" => 4,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            jpeg.clone(),
        );
        let resources = dictionary! { "XObject" => dictionary! { "Im1" => stream } };
        let (document, page) =
            image_document_with_resources(b"q 10 0 0 10 0 0 cm /Im1 Do Q", resources);
        (document, page, jpeg)
    }

    #[test]
    fn reading_back_an_unfiltered_rgb_images_bytes_round_trips_its_pixels() {
        // `fixture::image_resources`'s `Im1` is 2x2 `DeviceRGB`, uncompressed
        // — the no-filter branch.
        let (document, page) = image_document(b"q 10 0 0 10 0 0 cm /Im1 Do Q");
        let target = image_of(&document, 0);

        let png = image_source_bytes(&document, page, &target).expect("recoverable");

        assert_eq!(decoded_rgb_pixels(&png), vec![0u8; 12]);
    }

    #[test]
    fn reading_back_a_flate_decoded_rgb_images_bytes_round_trips_its_pixels() {
        let pixels: Vec<u8> = (0..2 * 3 * 3).map(|n| n as u8).collect();
        let (document, page) = flate_image_document(2, 3, pixels.clone());
        let target = image_of(&document, 0);

        let png = image_source_bytes(&document, page, &target).expect("recoverable");

        assert_eq!(decoded_rgb_pixels(&png), pixels);
    }

    #[test]
    fn reading_back_an_image_with_a_soft_mask_preserves_its_alpha() {
        let pixels = vec![10u8, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120];
        let (mut document, page) = flate_image_document(2, 2, pixels.clone());
        let target = image_of(&document, 0);

        let alpha = vec![255u8, 200, 100, 0];
        let mask_id = document.add_object(gray_mask_stream(2, 2, alpha.clone()));
        set_on_image_dict(
            &mut document,
            page,
            &target,
            "SMask",
            Object::Reference(mask_id),
        );

        let png = image_source_bytes(&document, page, &target).expect("recoverable");
        let decoded = image::load_from_memory(&png)
            .expect("decode png")
            .to_rgba8();

        assert_eq!(decoded_rgb_pixels(&png), pixels);
        assert_eq!(decoded.pixels().map(|p| p[3]).collect::<Vec<_>>(), alpha);
    }

    /// A `/SMask` whose plane is a different size than the image it masks
    /// cannot be paired with it sample for sample, so the read is refused
    /// rather than zipped into whichever of the two runs out first.
    #[test]
    fn a_soft_mask_that_does_not_match_its_base_image_is_refused() {
        let (mut document, page) = flate_image_document(2, 2, vec![0u8; 12]);
        let target = image_of(&document, 0);

        let mask_id = document.add_object(gray_mask_stream(4, 4, vec![255u8; 16]));
        set_on_image_dict(
            &mut document,
            page,
            &target,
            "SMask",
            Object::Reference(mask_id),
        );

        let error = image_source_bytes(&document, page, &target)
            .expect_err("a 4x4 mask does not describe a 2x2 image");

        assert!(matches!(error, EditError::ImageSourceNotRecoverable { .. }));
    }

    #[test]
    fn reading_back_a_dct_encoded_images_bytes_needs_no_re_encoding() {
        let (document, page, jpeg) = dct_image_document();
        let target = image_of(&document, 0);

        let bytes = image_source_bytes(&document, page, &target).expect("recoverable");

        assert_eq!(bytes, jpeg, "an already-JPEG stream is returned untouched");
    }

    /// The JPEG bytes alone are not the whole image when a `/SMask` carries
    /// its alpha: `replace_image_source` rebuilds `/SMask` from the
    /// replacement file's own channels, and a JPEG has none, so undoing with
    /// these bytes would restore the image opaque. Refused instead.
    #[test]
    fn a_dct_encoded_image_with_a_soft_mask_is_refused_rather_than_read_back_opaque() {
        let (mut document, page, _) = dct_image_document();
        let target = image_of(&document, 0);

        let mask_id = document.add_object(gray_mask_stream(4, 4, vec![128u8; 16]));
        set_on_image_dict(
            &mut document,
            page,
            &target,
            "SMask",
            Object::Reference(mask_id),
        );

        let error = image_source_bytes(&document, page, &target)
            .expect_err("a JPEG cannot carry a separate soft mask's alpha back");

        assert!(matches!(error, EditError::ImageSourceNotRecoverable { .. }));
    }

    /// `/Decode` remaps every sample on the way to the page — `[1 0 1 0 1 0]`
    /// paints a `DeviceRGB` image inverted — and nothing in the re-encoded
    /// PNG records that, so undo would restore the image in the wrong
    /// colours. Refused on both paths.
    #[test]
    fn a_decode_array_is_refused_rather_than_re_encoded_without_it() {
        let (mut document, page) = flate_image_document(2, 2, vec![7u8; 12]);
        let target = image_of(&document, 0);

        set_on_image_dict(
            &mut document,
            page,
            &target,
            "Decode",
            Object::Array(vec![
                1.into(),
                0.into(),
                1.into(),
                0.into(),
                1.into(),
                0.into(),
            ]),
        );

        let error = image_source_bytes(&document, page, &target)
            .expect_err("an inverting /Decode is not reproducible in a PNG");

        assert!(matches!(error, EditError::ImageSourceNotRecoverable { .. }));
    }

    /// `/Mask` is transparency the samples do not carry, and
    /// `replace_image_source` never writes one back, so reading the samples
    /// alone would hand undo an image that restores fully opaque.
    #[test]
    fn a_stencil_mask_is_refused_rather_than_dropping_its_transparency() {
        let (mut document, page, _) = dct_image_document();
        let target = image_of(&document, 0);

        let stencil_id = document.add_object(gray_mask_stream(4, 4, vec![0u8; 16]));
        set_on_image_dict(
            &mut document,
            page,
            &target,
            "Mask",
            Object::Reference(stencil_id),
        );

        let error = image_source_bytes(&document, page, &target)
            .expect_err("a /Mask's transparency does not survive the round trip");

        assert!(matches!(error, EditError::ImageSourceNotRecoverable { .. }));
    }

    /// A `/Width` that cannot be a width is refused at the dictionary rather
    /// than wrapped into a plausible `u32` and multiplied out into a sample
    /// count that overflows.
    #[test]
    fn a_nonsensical_declared_size_is_refused_rather_than_overflowing() {
        for (width, height) in [(-1i64, 2i64), (2, 0), (i64::from(u32::MAX) + 1, 2)] {
            let stream = Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => width,
                    "Height" => height,
                    "ColorSpace" => "DeviceRGB",
                    "BitsPerComponent" => 8,
                },
                vec![0u8; 12],
            );
            let resources = dictionary! { "XObject" => dictionary! { "Im1" => stream } };
            let (document, page) =
                image_document_with_resources(b"q 10 0 0 10 0 0 cm /Im1 Do Q", resources);
            let target = image_of(&document, 0);

            let error = image_source_bytes(&document, page, &target)
                .expect_err("a nonsensical size is not readable");

            assert!(
                matches!(error, EditError::ImageSourceNotRecoverable { .. }),
                "{width}x{height} must be refused, not read"
            );
        }
    }

    /// A stream that inflates far past the size its own dictionary declares
    /// is damaged or a decompression bomb; either way it is refused without
    /// materialising the whole expansion.
    #[test]
    fn a_stream_that_inflates_past_its_declared_size_is_refused() {
        // Declares 2x2 `DeviceRGB` (12 bytes) but inflates to 1 MiB.
        let (document, page) = flate_image_document(2, 2, vec![0u8; 1024 * 1024]);
        let target = image_of(&document, 0);

        let error = image_source_bytes(&document, page, &target)
            .expect_err("12 declared bytes must not decode into a megabyte");

        assert!(matches!(error, EditError::ImageSourceNotRecoverable { .. }));
    }

    #[test]
    fn an_unsupported_color_space_is_refused_rather_than_guessed_at() {
        let stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 2,
                "Height" => 2,
                "ColorSpace" => "DeviceCMYK",
                "BitsPerComponent" => 8,
            },
            vec![0u8; 16],
        );
        let resources = dictionary! { "XObject" => dictionary! { "Im1" => stream } };
        let (document, page) =
            image_document_with_resources(b"q 10 0 0 10 0 0 cm /Im1 Do Q", resources);
        let target = image_of(&document, 0);

        let error =
            image_source_bytes(&document, page, &target).expect_err("CMYK is not recoverable");

        assert!(matches!(error, EditError::ImageSourceNotRecoverable { .. }));
    }

    #[test]
    fn an_unsupported_filter_is_refused_rather_than_guessed_at() {
        let stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 2,
                "Height" => 2,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
                "Filter" => "CCITTFaxDecode",
            },
            vec![0u8; 4],
        );
        let resources = dictionary! { "XObject" => dictionary! { "Im1" => stream } };
        let (document, page) =
            image_document_with_resources(b"q 10 0 0 10 0 0 cm /Im1 Do Q", resources);
        let target = image_of(&document, 0);

        let error =
            image_source_bytes(&document, page, &target).expect_err("CCITT is not recoverable");

        assert!(matches!(error, EditError::ImageSourceNotRecoverable { .. }));
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
