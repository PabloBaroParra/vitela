//! Content-stream interpreter (T-152, second half).
//!
//! Walks the tokenized operators keeping just enough graphics and text state
//! to answer two questions for every painted item: *where is it on the page*
//! and *where are its bytes in the stream*. Everything else a full renderer
//! tracks — colour, clipping, blend modes — is irrelevant to editing and is
//! deliberately not modelled.
//!
//! ## Coordinate convention
//!
//! Boxes come out in **unrotated PDF user space**, the same space
//! `Annotation.rect` already uses. The page's `/Rotate` entry is a viewer
//! instruction, not a transform on the content, so applying it here would
//! double-rotate every box in a shell that already handles `/Rotate` for
//! annotations. That equivalence is pinned by a test.
//!
//! ## What v1 does not descend into
//!
//! - **Form XObjects.** Text painted inside a `/Subtype /Form` XObject is
//!   not reported, because its bytes live in a stream that may be shared by
//!   several pages — editing it would silently change all of them.
//! - **Inline images** (`BI`..`EI`). They have no resource name, so they do
//!   not fit `ImageItem`, and their payload is passed through opaquely.

use super::lexer::{Operand, SpannedOperation};
use super::matrix::Matrix;
use crate::encoding::{resolve_font, FontInfo};
use crate::error::EditError;
use lopdf::{Dictionary, Document, Object, ObjectId};
use pdf_document::{ContentItemId, ImageItem, PageContent, PageId, TextRun};
use std::collections::HashMap;
use std::ops::Range;

/// One of the page's content streams, decoded.
#[derive(Debug, Clone)]
pub struct PageStream {
    pub object_id: ObjectId,
    pub bytes: Vec<u8>,
    /// Whether the stream arrived compressed, carried from the decode rather
    /// than re-derived when writing.
    ///
    /// The writer has to put the stream back the way it found it, and asking
    /// the dictionary a second time is asking a question that has already
    /// been answered — by the code that had to resolve `/Filter` through
    /// indirection to answer it at all (see `crate::parse::filter`). Two
    /// readings of the same entry can disagree; one reading cannot.
    pub filtered: bool,
}

/// A text run plus where its bytes are, which is what [`crate::edit`] needs
/// and what `PageContent` deliberately does not carry.
#[derive(Debug, Clone)]
pub struct LocatedTextRun {
    pub run: TextRun,
    pub stream_index: usize,
    /// The whole show-text operation, operands included.
    pub operation_span: Range<usize>,
    /// Just the string or array operand being shown.
    pub operand_span: Range<usize>,
    pub operator: String,
    /// The `TJ` number that reproduces this run's advance without painting
    /// anything — what a deletion leaves behind so the text that follows on
    /// the same line does not slide backwards into the gap.
    ///
    /// Zero when the run advances nothing, or when the font size makes the
    /// conversion meaningless.
    pub advance_adjustment: f64,
}

/// An image plus where its `Do` is and what transform placed it.
#[derive(Debug, Clone)]
pub struct LocatedImage {
    pub item: ImageItem,
    pub stream_index: usize,
    /// The `/Name Do` operation.
    pub operation_span: Range<usize>,
    /// The CTM in effect when it was painted — the transform a move or
    /// resize has to correct.
    pub ctm_at_paint: Matrix,
}

/// Everything one page's content streams yielded.
#[derive(Debug, Clone)]
pub struct LocatedContent {
    pub streams: Vec<PageStream>,
    pub text_runs: Vec<LocatedTextRun>,
    pub images: Vec<LocatedImage>,
    /// The CTM left in effect once every stream has been walked.
    ///
    /// Content appended after that inherits it, so [`crate::insert`] has to
    /// cancel it out to place anything in page coordinates — a page that
    /// ends inside an unbalanced `q ... cm` is uncommon but perfectly legal,
    /// and appending blindly would put the new content somewhere else
    /// entirely.
    pub end_ctm: Matrix,
}

impl LocatedContent {
    /// The pure model to hand a shell — location data stays behind.
    ///
    /// `page` is stamped here rather than during the walk: interpreting a
    /// page's streams needs the page *object*, and the `PageId` naming it is
    /// the caller's knowledge (see [`crate::parse::read_located_content`]).
    /// Until this runs, every item carries the placeholder
    /// [`UNSTAMPED_PAGE`].
    pub fn page_content(&self, page: PageId) -> PageContent {
        PageContent {
            text_runs: self
                .text_runs
                .iter()
                .map(|located| TextRun {
                    page,
                    ..located.run.clone()
                })
                .collect(),
            images: self
                .images
                .iter()
                .map(|located| ImageItem {
                    page,
                    ..located.item.clone()
                })
                .collect(),
        }
    }

    pub fn text_run(&self, id: ContentItemId) -> Option<&LocatedTextRun> {
        self.text_runs.iter().find(|located| located.run.id == id)
    }

    pub fn image(&self, id: ContentItemId) -> Option<&LocatedImage> {
        self.images.iter().find(|located| located.item.id == id)
    }
}

/// The `PageId` items carry until [`LocatedContent::page_content`] stamps
/// the real one. Editing never reads it — an item's identity is its text,
/// font and box (see [`crate::edit`]) — so it is a placeholder, not a claim.
pub const UNSTAMPED_PAGE: PageId = PageId(0);

/// Ascent and descent assumed when the font descriptor gives none, in ems.
/// They sum to one em, which is what a line of text occupies.
const FALLBACK_ASCENT: f64 = 0.75;
const FALLBACK_DESCENT: f64 = -0.25;

/// The graphics and text state the interpreter tracks.
#[derive(Debug, Clone)]
struct State {
    ctm: Matrix,
    text_matrix: Matrix,
    line_matrix: Matrix,
    font: Option<String>,
    font_size: f64,
    leading: f64,
    char_spacing: f64,
    word_spacing: f64,
    horizontal_scale: f64,
    rise: f64,
}

impl Default for State {
    fn default() -> Self {
        State {
            ctm: Matrix::IDENTITY,
            text_matrix: Matrix::IDENTITY,
            line_matrix: Matrix::IDENTITY,
            font: None,
            font_size: 0.0,
            leading: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scale: 1.0,
            rise: 0.0,
        }
    }
}

/// Walks `streams` in order, as one logical stream — an array `/Contents`
/// is defined to behave that way, so `q`/`cm` set in one carries into the
/// next.
pub fn interpret(
    document: &Document,
    resources: &Dictionary,
    streams: &[PageStream],
) -> Result<LocatedContent, EditError> {
    let fonts = font_table(document, resources);
    let image_names = image_xobject_names(document, resources);

    let mut state = State::default();
    let mut stack: Vec<State> = Vec::new();
    let mut text_runs = Vec::new();
    let mut images = Vec::new();

    for (stream_index, stream) in streams.iter().enumerate() {
        for operation in super::lexer::tokenize(&stream.bytes)? {
            apply_operation(
                &operation,
                &mut state,
                &mut stack,
                &Context {
                    stream_index,
                    fonts: &fonts,
                    image_names: &image_names,
                },
                &mut text_runs,
                &mut images,
            );
        }
    }

    Ok(LocatedContent {
        streams: streams.to_vec(),
        text_runs,
        images,
        end_ctm: state.ctm,
    })
}

struct Context<'a> {
    stream_index: usize,
    fonts: &'a HashMap<String, FontInfo>,
    image_names: &'a [String],
}

fn apply_operation(
    operation: &SpannedOperation,
    state: &mut State,
    stack: &mut Vec<State>,
    context: &Context<'_>,
    text_runs: &mut Vec<LocatedTextRun>,
    images: &mut Vec<LocatedImage>,
) {
    let operands = &operation.operands;

    match operation.operator.as_str() {
        "q" => stack.push(state.clone()),
        "Q" => {
            if let Some(restored) = stack.pop() {
                *state = restored;
            }
        }
        "cm" => {
            if let Some(matrix) = matrix_from(operands) {
                state.ctm = matrix.then(state.ctm);
            }
        }
        "BT" => {
            state.text_matrix = Matrix::IDENTITY;
            state.line_matrix = Matrix::IDENTITY;
        }
        "Tf" => {
            if let Some(Operand::Name(name)) = operands.first() {
                state.font = Some(name.clone());
            }
            state.font_size = operands.get(1).and_then(Operand::as_f64).unwrap_or(0.0);
        }
        "TL" => state.leading = operands.first().and_then(Operand::as_f64).unwrap_or(0.0),
        "Tc" => state.char_spacing = operands.first().and_then(Operand::as_f64).unwrap_or(0.0),
        "Tw" => state.word_spacing = operands.first().and_then(Operand::as_f64).unwrap_or(0.0),
        "Tz" => {
            state.horizontal_scale = operands
                .first()
                .and_then(Operand::as_f64)
                .map_or(1.0, |percent| percent / 100.0)
        }
        "Ts" => state.rise = operands.first().and_then(Operand::as_f64).unwrap_or(0.0),
        "Td" => translate_line(state, operands),
        "TD" => {
            // `TD` is `Td` with a side effect: it also sets the leading to
            // the negated vertical displacement.
            if let Some(ty) = operands.get(1).and_then(Operand::as_f64) {
                state.leading = -ty;
            }
            translate_line(state, operands);
        }
        "Tm" => {
            if let Some(matrix) = matrix_from(operands) {
                state.text_matrix = matrix;
                state.line_matrix = matrix;
            }
        }
        "T*" => next_line(state),
        "Tj" | "TJ" => {
            show_text(operation, state, context, text_runs, 0);
        }
        "'" => {
            next_line(state);
            show_text(operation, state, context, text_runs, 0);
        }
        "\"" => {
            // `aw ac string "` — word and char spacing, then a new line.
            state.word_spacing = operands.first().and_then(Operand::as_f64).unwrap_or(0.0);
            state.char_spacing = operands.get(1).and_then(Operand::as_f64).unwrap_or(0.0);
            next_line(state);
            show_text(operation, state, context, text_runs, 2);
        }
        "Do" => {
            if let Some(Operand::Name(name)) = operands.first() {
                if context.image_names.iter().any(|known| known == name) {
                    images.push(LocatedImage {
                        item: ImageItem {
                            id: ContentItemId(images.len() as u64),
                            page: UNSTAMPED_PAGE,
                            bbox: state.ctm.bounding_box(0.0, 0.0, 1.0, 1.0),
                            resource_xobject_name: name.clone(),
                        },
                        stream_index: context.stream_index,
                        operation_span: operation.span.clone(),
                        ctm_at_paint: state.ctm,
                    });
                }
            }
        }
        _ => {}
    }
}

fn translate_line(state: &mut State, operands: &[Operand]) {
    let tx = operands.first().and_then(Operand::as_f64).unwrap_or(0.0);
    let ty = operands.get(1).and_then(Operand::as_f64).unwrap_or(0.0);
    state.line_matrix = Matrix::translate(tx, ty).then(state.line_matrix);
    state.text_matrix = state.line_matrix;
}

fn next_line(state: &mut State) {
    let leading = state.leading;
    state.line_matrix = Matrix::translate(0.0, -leading).then(state.line_matrix);
    state.text_matrix = state.line_matrix;
}

fn matrix_from(operands: &[Operand]) -> Option<Matrix> {
    if operands.len() < 6 {
        return None;
    }
    let values: Option<Vec<f64>> = operands[..6].iter().map(Operand::as_f64).collect();
    let values = values?;
    Some(Matrix::new(
        values[0], values[1], values[2], values[3], values[4], values[5],
    ))
}

/// Records the run painted by a show-text operator and advances the text
/// matrix past it.
///
/// `operand_index` is where the shown string sits among the operands — 0 for
/// `Tj`/`TJ`/`'`, but 2 for `"`, whose first two operands are spacings.
fn show_text(
    operation: &SpannedOperation,
    state: &mut State,
    context: &Context<'_>,
    text_runs: &mut Vec<LocatedTextRun>,
    operand_index: usize,
) {
    let Some(operand) = operation.operands.get(operand_index) else {
        return;
    };
    let Some(span) = operation.operand_spans.get(operand_index).cloned() else {
        return;
    };
    let Some(font_name) = state.font.clone() else {
        // Showing text with no font set is malformed; there is nothing to
        // decode it with, so it is passed over rather than guessed at.
        return;
    };
    let Some(font) = context.fonts.get(&font_name) else {
        return;
    };

    let (codes, kern_adjustment) = collect_codes(operand);
    let advance = run_advance(state, font, &codes, kern_adjustment);

    if codes.is_empty() {
        // A `TJ` carrying only adjustments paints nothing but still moves
        // the text matrix — that is exactly what a deleted run leaves
        // behind, so skipping the advance here would let everything after
        // it on the line slide backwards.
        state.text_matrix = Matrix::translate(advance, 0.0).then(state.text_matrix);
        return;
    }

    let text = font.decode(&codes);
    let bbox = run_bounding_box(state, advance);

    text_runs.push(LocatedTextRun {
        run: TextRun {
            id: ContentItemId(text_runs.len() as u64),
            page: UNSTAMPED_PAGE,
            bbox,
            resource_font_name: font_name,
            font_kind: font.kind,
            text,
        },
        stream_index: context.stream_index,
        operation_span: operation.span.clone(),
        operand_span: span,
        operator: operation.operator.clone(),
        advance_adjustment: advance_adjustment(state, advance),
    });

    state.text_matrix = Matrix::translate(advance, 0.0).then(state.text_matrix);
}

/// Flattens a show-text operand to its code bytes, plus the total kerning
/// adjustment a `TJ` array applies (in thousandths of an em, positive values
/// moving text left).
fn collect_codes(operand: &Operand) -> (Vec<u8>, f64) {
    match operand {
        Operand::LiteralString(bytes) | Operand::HexString(bytes) => (bytes.clone(), 0.0),
        Operand::Array(items) => {
            let mut codes = Vec::new();
            let mut adjustment = 0.0;
            for item in items {
                match item {
                    Operand::LiteralString(bytes) | Operand::HexString(bytes) => {
                        codes.extend_from_slice(bytes)
                    }
                    other => adjustment += other.as_f64().unwrap_or(0.0),
                }
            }
            (codes, adjustment)
        }
        _ => (Vec::new(), 0.0),
    }
}

/// Horizontal advance of the run in text space, per PDF 32000-1 9.4.4.
fn run_advance(state: &State, font: &FontInfo, codes: &[u8], kern_adjustment: f64) -> f64 {
    let glyph_width = font.width_of(codes) * state.font_size;
    let spacing = state.char_spacing * codes.len() as f64;
    let word_spacing =
        state.word_spacing * codes.iter().filter(|&&code| code == b' ').count() as f64;
    let kerning = kern_adjustment / 1000.0 * state.font_size;

    (glyph_width + spacing + word_spacing - kerning) * state.horizontal_scale
}

/// Inverts the `TJ` displacement rule (PDF 32000-1 9.4.3): an adjustment
/// `adj` moves the text matrix by `-adj/1000 × size × scale`, so the
/// adjustment reproducing a known advance is its negation, scaled back up.
fn advance_adjustment(state: &State, advance: f64) -> f64 {
    let scale = state.font_size * state.horizontal_scale;
    if scale.abs() < 1e-9 {
        return 0.0;
    }
    -advance * 1000.0 / scale
}

/// The run's box in page space: a rectangle one em tall, sitting on the
/// baseline, pushed through the text matrix and then the CTM.
fn run_bounding_box(state: &State, advance: f64) -> pdf_document::Rect {
    let ascent = FALLBACK_ASCENT * state.font_size;
    let descent = FALLBACK_DESCENT * state.font_size;
    let transform = state.text_matrix.then(state.ctm);

    transform.bounding_box(0.0, descent + state.rise, advance, ascent - descent)
}

/// Resolves every font in `/Resources /Font` up front: a page has a handful,
/// and resolving lazily would mean re-reading the same dictionary once per
/// run.
fn font_table(document: &Document, resources: &Dictionary) -> HashMap<String, FontInfo> {
    let mut table = HashMap::new();

    let Some(fonts) = sub_dictionary(document, resources, b"Font") else {
        return table;
    };

    for (name, value) in fonts.iter() {
        let name = String::from_utf8_lossy(name).into_owned();
        let font_dict = match dereference(document, value) {
            Some(Object::Dictionary(dict)) => dict.clone(),
            _ => continue,
        };
        if let Ok(info) = resolve_font(document, &font_dict, &name) {
            table.insert(name, info);
        }
    }

    table
}

/// The names in `/Resources /XObject` whose `/Subtype` is `/Image`. Form
/// XObjects are excluded on purpose — see the module docs.
fn image_xobject_names(document: &Document, resources: &Dictionary) -> Vec<String> {
    let Some(xobjects) = sub_dictionary(document, resources, b"XObject") else {
        return Vec::new();
    };

    xobjects
        .iter()
        .filter(|(_, value)| {
            let subtype = match dereference(document, value) {
                Some(Object::Stream(stream)) => stream.dict.get(b"Subtype").ok().cloned(),
                Some(Object::Dictionary(dict)) => dict.get(b"Subtype").ok().cloned(),
                _ => None,
            };
            subtype
                .and_then(|object| object.as_name().ok().map(<[u8]>::to_vec))
                .is_some_and(|name| name == b"Image")
        })
        .map(|(name, _)| String::from_utf8_lossy(name).into_owned())
        .collect()
}

fn sub_dictionary(document: &Document, parent: &Dictionary, key: &[u8]) -> Option<Dictionary> {
    match dereference(document, parent.get(key).ok()?) {
        Some(Object::Dictionary(dict)) => Some(dict.clone()),
        _ => None,
    }
}

fn dereference<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => document.get_object(*id).ok(),
        direct => Some(direct),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;
    use crate::parse::read_located_content;
    use pdf_document::FontKind;

    fn located(content: &[u8], resources: Dictionary) -> LocatedContent {
        let (document, page_object) = fixture::document_with_content(content, resources);
        read_located_content(&document, page_object).expect("readable page")
    }

    fn text(content: &[u8]) -> LocatedContent {
        located(content, fixture::helvetica_resources())
    }

    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-6
    }

    #[test]
    fn a_show_text_operator_becomes_a_text_run() {
        let content = located(
            b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET",
            fixture::helvetica_resources(),
        );

        assert_eq!(content.text_runs.len(), 1);
        let run = &content.text_runs[0].run;
        assert_eq!(run.text, "Hello");
        assert_eq!(run.resource_font_name, "F1");
        assert_eq!(run.font_kind, FontKind::Standard14);
    }

    #[test]
    fn the_run_box_sits_on_the_baseline_set_by_td() {
        let content = text(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let bbox = content.text_runs[0].run.bbox;

        assert!(close(bbox.x, 100.0), "left edge at the text position");
        assert!(
            close(bbox.y, 700.0 - 3.0),
            "descender hangs below the baseline"
        );
        assert!(close(bbox.height, 12.0), "one em tall");
        // No /Widths on a standard-14 font, so the advance comes from this
        // crate's real Helvetica AFM widths (H722+e556+l222+l222+o556 =
        // 2278 units, at 12pt).
        assert!(close(bbox.width, 27.336));
    }

    #[test]
    fn a_text_matrix_positions_the_run_absolutely() {
        let content = text(b"BT /F1 10 Tf 1 0 0 1 50 400 Tm (Hi) Tj ET");
        let bbox = content.text_runs[0].run.bbox;

        assert!(close(bbox.x, 50.0));
        assert!(close(bbox.y, 400.0 - 2.5));
    }

    #[test]
    fn the_ctm_transforms_the_run_box() {
        let content = text(b"q 2 0 0 2 0 0 cm BT /F1 12 Tf 100 700 Td (Hello) Tj ET Q");
        let bbox = content.text_runs[0].run.bbox;

        assert!(close(bbox.x, 200.0), "the ctm scale reaches the box");
        assert!(
            close(bbox.width, 54.672),
            "the 2x ctm doubles the AFM advance too"
        );
    }

    #[test]
    fn q_and_q_restore_the_previous_ctm() {
        let content = text(b"q 2 0 0 2 0 0 cm Q BT /F1 12 Tf 100 700 Td (Hello) Tj ET");

        assert!(close(content.text_runs[0].run.bbox.x, 100.0));
    }

    #[test]
    fn nested_cm_operators_compose() {
        let content = text(b"2 0 0 2 0 0 cm 1 0 0 1 10 0 cm BT /F1 12 Tf 0 700 Td (Hi) Tj ET");

        // The inner translation happens in the already-scaled space: 10 * 2.
        assert!(close(content.text_runs[0].run.bbox.x, 20.0));
    }

    #[test]
    fn a_tj_array_reads_as_one_run() {
        let content = text(b"BT /F1 12 Tf 0 700 Td [(He) -20 (llo)] TJ ET");

        assert_eq!(content.text_runs.len(), 1);
        assert_eq!(content.text_runs[0].run.text, "Hello");
    }

    #[test]
    fn kerning_inside_a_tj_array_narrows_the_run() {
        let plain = text(b"BT /F1 12 Tf 0 700 Td [(Hello)] TJ ET");
        let kerned = text(b"BT /F1 12 Tf 0 700 Td [(He) 1000 (llo)] TJ ET");

        assert!(
            kerned.text_runs[0].run.bbox.width < plain.text_runs[0].run.bbox.width,
            "a positive TJ adjustment pulls the following text left"
        );
    }

    #[test]
    fn consecutive_runs_advance_along_the_line() {
        let content = text(b"BT /F1 12 Tf 100 700 Td (ab) Tj (cd) Tj ET");

        assert_eq!(content.text_runs.len(), 2);
        assert!(close(content.text_runs[0].run.bbox.x, 100.0));
        assert!(
            // "ab": a556 + b556 = 1112 units, at 12pt = 13.344pt advance.
            close(content.text_runs[1].run.bbox.x, 113.344),
            "the second run starts where the first ended"
        );
    }

    #[test]
    fn the_quote_operators_move_to_the_next_line_first() {
        let content = text(b"BT /F1 10 Tf 14 TL 0 700 Td (first) ' (second) '");

        assert_eq!(content.text_runs.len(), 2);
        assert!(close(content.text_runs[0].run.bbox.y, 700.0 - 14.0 - 2.5));
        assert!(close(content.text_runs[1].run.bbox.y, 700.0 - 28.0 - 2.5));
    }

    #[test]
    fn the_double_quote_operator_shows_its_third_operand() {
        let content = text(b"BT /F1 10 Tf 12 TL 0 700 Td 5 1 (spaced) \" ET");

        assert_eq!(content.text_runs.len(), 1);
        assert_eq!(content.text_runs[0].run.text, "spaced");
    }

    #[test]
    fn td_sets_the_leading_for_later_lines() {
        let content = text(b"BT /F1 10 Tf 0 700 Td 0 -20 TD (a) Tj T* (b) Tj ET");

        assert!(close(content.text_runs[0].run.bbox.y, 680.0 - 2.5));
        assert!(
            close(content.text_runs[1].run.bbox.y, 660.0 - 2.5),
            "T* reuses the leading TD established"
        );
    }

    #[test]
    fn an_image_xobject_becomes_an_image_item() {
        let content = located(
            b"q 100 0 0 50 10 20 cm /Im1 Do Q",
            fixture::image_resources(),
        );

        assert_eq!(content.images.len(), 1);
        let item = &content.images[0].item;
        assert_eq!(item.resource_xobject_name, "Im1");
        assert!(close(item.bbox.x, 10.0) && close(item.bbox.y, 20.0));
        assert!(close(item.bbox.width, 100.0) && close(item.bbox.height, 50.0));
    }

    #[test]
    fn a_rotated_image_reports_a_box_that_covers_it() {
        let content = located(
            b"q 0 50 -100 0 10 20 cm /Im1 Do Q",
            fixture::image_resources(),
        );
        let bbox = content.images[0].item.bbox;

        assert!(close(bbox.width, 100.0) && close(bbox.height, 50.0));
    }

    /// Form XObjects are skipped, and skipping is a decision rather than an
    /// oversight — their stream can be shared by other pages.
    #[test]
    fn a_form_xobject_is_not_reported_as_an_image() {
        use lopdf::dictionary;
        let resources = dictionary! {
            "XObject" => dictionary! {
                "Fm1" => lopdf::Stream::new(
                    dictionary! { "Type" => "XObject", "Subtype" => "Form" },
                    b"BT ET".to_vec(),
                ),
            },
        };

        assert!(located(b"q /Fm1 Do Q", resources).images.is_empty());
    }

    #[test]
    fn an_inline_image_is_not_reported_as_an_item() {
        let content = located(
            b"q BI /W 2 /H 2 /CS /G /BPC 8 ID \x00\x01\x02\x03 EI Q",
            fixture::image_resources(),
        );

        assert!(
            content.images.is_empty(),
            "an inline image has no resource name to target"
        );
    }

    #[test]
    fn text_and_image_ids_are_numbered_independently() {
        let content = located(
            b"q 10 0 0 10 0 0 cm /Im1 Do Q BT /F1 12 Tf (a) Tj ET",
            fixture::text_and_image_resources(),
        );

        assert_eq!(content.text_runs[0].run.id, ContentItemId(0));
        assert_eq!(content.images[0].item.id, ContentItemId(0));
    }

    #[test]
    fn ids_follow_stream_order() {
        let content = text(b"BT /F1 12 Tf (a) Tj (b) Tj (c) Tj ET");

        assert_eq!(
            content
                .text_runs
                .iter()
                .map(|located| located.run.id)
                .collect::<Vec<_>>(),
            vec![ContentItemId(0), ContentItemId(1), ContentItemId(2)]
        );
    }

    /// An array `/Contents` is one logical stream, so state set in the first
    /// part governs the second. A parser that reset between them would place
    /// everything after the first stream wrongly.
    #[test]
    fn graphics_state_carries_from_one_content_stream_to_the_next() {
        let (document, page_object) = fixture::document_with_streams(
            &[b"q 2 0 0 2 0 0 cm", b"BT /F1 12 Tf 100 700 Td (Hi) Tj ET Q"],
            fixture::helvetica_resources(),
        );
        let content = read_located_content(&document, page_object).expect("readable page");

        assert!(close(content.text_runs[0].run.bbox.x, 200.0));
        assert_eq!(content.text_runs[0].stream_index, 1);
    }

    /// `/Rotate` is a viewer instruction, not a transform on the content.
    /// Baking it in here would double-rotate every box in a shell that
    /// already applies it — the same way it does for annotation rects.
    #[test]
    fn the_page_rotate_entry_does_not_move_the_boxes() {
        let (mut document, page_object) = fixture::document_with_content(
            b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET",
            fixture::helvetica_resources(),
        );
        let unrotated = read_located_content(&document, page_object)
            .expect("readable page")
            .text_runs[0]
            .run
            .bbox;

        document
            .get_dictionary_mut(page_object)
            .expect("page dictionary")
            .set("Rotate", 90);
        let rotated = read_located_content(&document, page_object)
            .expect("readable page")
            .text_runs[0]
            .run
            .bbox;

        assert_eq!(unrotated, rotated);
    }

    #[test]
    fn text_shown_with_no_font_set_is_skipped_rather_than_guessed() {
        assert!(text(b"BT 100 700 Td (orphan) Tj ET").text_runs.is_empty());
    }

    #[test]
    fn located_runs_point_back_at_the_bytes_that_produced_them() {
        let source = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
        let content = text(source);
        let located = &content.text_runs[0];

        assert_eq!(&source[located.operand_span.clone()], b"(Hello)");
        assert_eq!(&source[located.operation_span.clone()], b"(Hello) Tj");
        assert_eq!(located.operator, "Tj");
        assert_eq!(located.stream_index, 0);
    }

    #[test]
    fn page_content_drops_the_location_data() {
        let content = text(b"BT /F1 12 Tf (a) Tj ET").page_content(PageId(0));

        assert_eq!(content.text_runs.len(), 1);
        assert_eq!(
            content.text_run(ContentItemId(0)).expect("present").text,
            "a"
        );
    }

    /// The walk cannot know which `PageId` names the object it was handed,
    /// so the stamp happens on the way out — and it has to reach every item,
    /// or a shell hit-testing a run would look for it on the wrong page.
    #[test]
    fn page_content_stamps_the_page_every_item_belongs_to() {
        let content = located(
            b"q 10 0 0 10 0 0 cm /Im1 Do Q BT /F1 12 Tf (a) Tj ET",
            fixture::text_and_image_resources(),
        )
        .page_content(PageId(7));

        assert_eq!(content.text_runs[0].page, PageId(7));
        assert_eq!(content.images[0].page, PageId(7));
    }
}
