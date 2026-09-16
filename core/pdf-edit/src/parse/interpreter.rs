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
//! - **Inline images** (`BI`..`EI`). They have no resource name, so they do
//!   not fit `ImageItem`, and their payload is passed through opaquely.

use super::lexer::{Operand, SpannedOperation};
use super::matrix::Matrix;
use crate::encoding::{resolve_font, FontInfo};
use crate::error::EditError;
use lopdf::{Dictionary, Document, Object, ObjectId};
use pdf_document::{ContentItemId, ImageItem, PageContent, PageId, TextRun};
use std::collections::{HashMap, HashSet};
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

/// The matrices and advance that put a run where it is — what a
/// reposition ([`crate::move_text_run`]) has to work from, and what the
/// `TextRun` model deliberately does not carry.
///
/// A move rewrites the run's text matrix and then has to put the text state
/// back exactly as the original operation left it, so both matrices are
/// needed, not just the one that positioned the glyphs: `text_matrix` is
/// where this run painted, `line_matrix` is what the *next* relative
/// positioning operator (`Td`, `T*`, `'`) will be measured from.
#[derive(Debug, Clone, Copy)]
pub struct TextPlacement {
    /// `Tm` in effect when the run was shown.
    pub text_matrix: Matrix,
    /// `Tlm` in effect when the run was shown. Equal to `text_matrix` for
    /// the first run after a positioning operator, and behind it by the
    /// accumulated advance for every run after that on the same line.
    pub line_matrix: Matrix,
    /// The CTM in effect when the run was shown — what turns text space
    /// into page space, and therefore what a page-space displacement has to
    /// be pulled back through.
    pub ctm: Matrix,
    /// The `Tf` size in effect for this run. Font substitution needs it to
    /// select a temporary font without changing the run's text geometry.
    pub font_size: f64,
    /// The run's own horizontal advance, in text space.
    pub advance: f64,
    /// `Tfs × Th` — the factor that turns a `TJ` adjustment into a
    /// text-space displacement (PDF 32000-1 9.4.3). Zero when the font size
    /// or horizontal scale is degenerate, which is exactly when no `TJ`
    /// number can reproduce a displacement at all.
    pub advance_scale: f64,
}

/// A text run plus where its bytes are, which is what [`crate::edit`] needs
/// and what `PageContent` deliberately does not carry.
#[derive(Debug, Clone)]
pub struct LocatedTextRun {
    pub run: TextRun,
    pub stream_index: usize,
    /// The Form invocations traversed from the page to this stream.
    pub form_path: Vec<FormStep>,
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
    /// Where the text state stood when this run painted.
    pub placement: TextPlacement,
}

/// An image plus where its `Do` is and what transform placed it.
#[derive(Debug, Clone)]
pub struct LocatedImage {
    pub item: ImageItem,
    /// The image XObject the name resolved to, **in the scope that painted
    /// it** — `None` when the resource is a stream written directly into the
    /// dictionary, which has no id for a caller to address.
    ///
    /// Resolved here because here is the only place that can: a form's own
    /// `/Resources` replace its caller's, so the same `/Im0` means one image
    /// on the page and another inside a form, and only the walk that entered
    /// the form knows which table was in effect.
    pub xobject: Option<ObjectId>,
    pub stream_index: usize,
    /// The Form invocations traversed from the page to this stream.
    pub form_path: Vec<FormStep>,
    /// The `/Name Do` operation.
    pub operation_span: Range<usize>,
    /// The CTM in effect when it was painted — the transform a move or
    /// resize has to correct.
    pub ctm_at_paint: Matrix,
}

/// One Form XObject invocation on the path to located content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormStep {
    pub object_id: ObjectId,
    pub resource_name: String,
    pub stream_index: usize,
    pub operation_span: Range<usize>,
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
    let mut state = State::default();
    let mut text_runs = Vec::new();
    let mut images = Vec::new();
    let mut active_forms = HashSet::new();
    let mut decode_budget = super::filter::MAX_PAGE_CONTENT_BYTES;
    interpret_streams(
        document,
        resources,
        streams,
        &mut state,
        &[],
        &mut active_forms,
        &mut decode_budget,
        &mut text_runs,
        &mut images,
    )?;

    Ok(LocatedContent {
        streams: streams.to_vec(),
        text_runs,
        images,
        end_ctm: state.ctm,
    })
}

#[allow(clippy::too_many_arguments)]
fn interpret_streams(
    document: &Document,
    resources: &Dictionary,
    streams: &[PageStream],
    state: &mut State,
    form_path: &[FormStep],
    active_forms: &mut HashSet<ObjectId>,
    decode_budget: &mut usize,
    text_runs: &mut Vec<LocatedTextRun>,
    images: &mut Vec<LocatedImage>,
) -> Result<(), EditError> {
    let fonts = font_table(document, resources);
    let xobjects = xobject_table(document, resources);
    let mut stack = Vec::new();

    for (stream_index, stream) in streams.iter().enumerate() {
        for operation in super::lexer::tokenize(&stream.bytes)? {
            let form = apply_operation(
                &operation,
                state,
                &mut stack,
                &Context {
                    stream_index,
                    fonts: &fonts,
                    xobjects: &xobjects,
                    form_path,
                },
                text_runs,
                images,
            );
            if let Some((name, object_id)) = form {
                interpret_form(
                    document,
                    resources,
                    state.ctm,
                    FormStep {
                        object_id,
                        resource_name: name,
                        stream_index,
                        operation_span: operation.span.clone(),
                    },
                    form_path,
                    active_forms,
                    decode_budget,
                    text_runs,
                    images,
                )?;
            }
        }
    }
    Ok(())
}

/// How many `/Form Do` invocations may nest before the reader refuses.
///
/// Real files nest a handful of levels; this is the same generous ceiling
/// `MAX_INHERITANCE_DEPTH` puts on a `/Parent` chain. It exists because the
/// descent below is recursive and a chain of distinct forms is bounded by
/// nothing else — the cycle guard only catches re-entry, and a level costs
/// too few bytes for the decode budget to reach.
pub(crate) const MAX_FORM_DEPTH: usize = 32;

#[allow(clippy::too_many_arguments)]
fn interpret_form(
    document: &Document,
    parent_resources: &Dictionary,
    caller_ctm: Matrix,
    step: FormStep,
    parent_path: &[FormStep],
    active_forms: &mut HashSet<ObjectId>,
    decode_budget: &mut usize,
    text_runs: &mut Vec<LocatedTextRun>,
    images: &mut Vec<LocatedImage>,
) -> Result<(), EditError> {
    if parent_path.len() >= MAX_FORM_DEPTH {
        return Err(EditError::FormNestingTooDeep {
            limit: MAX_FORM_DEPTH,
        });
    }

    let object_id = step.object_id;
    if !active_forms.insert(object_id) {
        return Ok(());
    }

    let result = (|| {
        let stream = document.get_object(step.object_id)?.as_stream()?;
        let decoded = super::filter::decode(document, stream, step.object_id, *decode_budget)?;
        *decode_budget = decode_budget.saturating_sub(decoded.bytes.len());
        let resources = stream
            .dict
            .get(b"Resources")
            .ok()
            .and_then(|object| dereference(document, object))
            .and_then(|object| object.as_dict().ok())
            .cloned()
            .unwrap_or_else(|| parent_resources.clone());
        let matrix = stream
            .dict
            .get(b"Matrix")
            .ok()
            .and_then(object_matrix)
            .unwrap_or(Matrix::IDENTITY);
        let mut state = State {
            ctm: matrix.then(caller_ctm),
            ..State::default()
        };
        let mut path = parent_path.to_vec();
        path.push(step);
        interpret_streams(
            document,
            &resources,
            &[PageStream {
                object_id: path.last().expect("path contains the form").object_id,
                bytes: decoded.bytes,
                filtered: decoded.filtered,
            }],
            &mut state,
            &path,
            active_forms,
            decode_budget,
            text_runs,
            images,
        )
    })();

    active_forms.remove(&object_id);
    result
}

struct Context<'a> {
    stream_index: usize,
    fonts: &'a HashMap<String, FontInfo>,
    xobjects: &'a HashMap<String, XObjectKind>,
    form_path: &'a [FormStep],
}

#[derive(Debug, Clone, Copy)]
enum XObjectKind {
    Image(Option<ObjectId>),
    Form(ObjectId),
}

fn apply_operation(
    operation: &SpannedOperation,
    state: &mut State,
    stack: &mut Vec<State>,
    context: &Context<'_>,
    text_runs: &mut Vec<LocatedTextRun>,
    images: &mut Vec<LocatedImage>,
) -> Option<(String, ObjectId)> {
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
                match context.xobjects.get(name) {
                    Some(XObjectKind::Image(xobject)) => images.push(LocatedImage {
                        item: ImageItem {
                            id: ContentItemId(images.len() as u64),
                            page: UNSTAMPED_PAGE,
                            bbox: state.ctm.bounding_box(0.0, 0.0, 1.0, 1.0),
                            resource_xobject_name: name.clone(),
                        },
                        xobject: *xobject,
                        stream_index: context.stream_index,
                        form_path: context.form_path.to_vec(),
                        operation_span: operation.span.clone(),
                        ctm_at_paint: state.ctm,
                    }),
                    Some(XObjectKind::Form(object_id)) => return Some((name.clone(), *object_id)),
                    None => {}
                }
            }
        }
        _ => {}
    }
    None
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
        form_path: context.form_path.to_vec(),
        operation_span: operation.span.clone(),
        operand_span: span,
        operator: operation.operator.clone(),
        advance_adjustment: advance_adjustment(state, advance),
        placement: TextPlacement {
            text_matrix: state.text_matrix,
            line_matrix: state.line_matrix,
            ctm: state.ctm,
            font_size: state.font_size,
            advance,
            advance_scale: state.font_size * state.horizontal_scale,
        },
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

fn xobject_table(document: &Document, resources: &Dictionary) -> HashMap<String, XObjectKind> {
    let Some(xobjects) = sub_dictionary(document, resources, b"XObject") else {
        return HashMap::new();
    };

    xobjects
        .iter()
        .filter_map(|(name, value)| {
            let stream = dereference(document, value)?.as_stream().ok()?;
            let subtype = stream.dict.get(b"Subtype").ok()?.as_name().ok()?;
            let kind = match subtype {
                b"Image" => XObjectKind::Image(value.as_reference().ok()),
                b"Form" => XObjectKind::Form(value.as_reference().ok()?),
                _ => return None,
            };
            Some((String::from_utf8_lossy(name).into_owned(), kind))
        })
        .collect()
}

fn object_matrix(object: &Object) -> Option<Matrix> {
    let Object::Array(values) = object else {
        return None;
    };
    if values.len() != 6 {
        return None;
    }
    let values: Option<Vec<f64>> = values
        .iter()
        .map(|value| match value {
            Object::Integer(value) => Some(*value as f64),
            Object::Real(value) => Some((*value).into()),
            _ => None,
        })
        .collect();
    let values = values?;
    Some(Matrix::new(
        values[0], values[1], values[2], values[3], values[4], values[5],
    ))
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

    fn form_document(form_content: &[u8], form_entries: Dictionary) -> (Document, ObjectId) {
        use lopdf::{dictionary, Stream};

        let (mut document, page) =
            fixture::document_with_content(b"q 2 0 0 2 10 20 cm /Fm1 Do Q", Dictionary::new());
        let form_id = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
                "Resources" => form_entries,
            },
            form_content.to_vec(),
        ));
        document
            .get_dictionary_mut(page)
            .expect("page dictionary")
            .set(
                "Resources",
                dictionary! { "XObject" => dictionary! { "Fm1" => form_id } },
            );
        (document, page)
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

    #[test]
    fn text_inside_a_form_is_reported_with_its_invocation_path() {
        let (document, page) = form_document(
            b"BT /F1 12 Tf 5 7 Td (inside) Tj ET",
            fixture::helvetica_resources(),
        );

        let content = read_located_content(&document, page).expect("readable form");

        assert_eq!(content.text_runs.len(), 1);
        assert_eq!(content.text_runs[0].run.text, "inside");
        assert_eq!(content.text_runs[0].form_path.len(), 1);
        assert_eq!(content.text_runs[0].form_path[0].resource_name, "Fm1");
        assert!(close(content.text_runs[0].run.bbox.x, 20.0));
        assert!(close(content.text_runs[0].run.bbox.y, 28.0));
    }

    #[test]
    fn a_form_without_resources_uses_its_callers_resources() {
        use lopdf::{dictionary, Stream};

        let (mut document, page) = fixture::document_with_content(b"/Fm1 Do", Dictionary::new());
        let form = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            },
            b"BT /F1 12 Tf (inherited) Tj ET".to_vec(),
        ));
        let mut resources = fixture::helvetica_resources();
        resources.set("XObject", dictionary! { "Fm1" => form });
        document
            .get_dictionary_mut(page)
            .expect("page dictionary")
            .set("Resources", resources);

        let content = read_located_content(&document, page).expect("readable form");

        assert_eq!(content.text_runs[0].run.text, "inherited");
    }

    #[test]
    fn a_form_matrix_composes_with_the_invocation_ctm() {
        let (mut document, page) = form_document(
            b"BT /F1 10 Tf 1 0 0 1 1 0 Tm (x) Tj ET",
            fixture::helvetica_resources(),
        );
        let form_id = document
            .get_dictionary(page)
            .expect("page dictionary")
            .get(b"Resources")
            .expect("resources")
            .as_dict()
            .expect("resources dictionary")
            .get(b"XObject")
            .expect("xobjects")
            .as_dict()
            .expect("xobject dictionary")
            .get(b"Fm1")
            .expect("form")
            .as_reference()
            .expect("form reference");
        document
            .get_object_mut(form_id)
            .expect("form object")
            .as_stream_mut()
            .expect("form stream")
            .dict
            .set(
                "Matrix",
                vec![1.into(), 0.into(), 0.into(), 1.into(), 3.into(), 0.into()],
            );

        let content = read_located_content(&document, page).expect("readable form");

        assert!(close(content.text_runs[0].run.bbox.x, 18.0));
    }

    #[test]
    fn the_same_form_invoked_twice_produces_two_distinct_paths() {
        let (mut document, page) =
            form_document(b"BT /F1 10 Tf (x) Tj ET", fixture::helvetica_resources());
        let contents_id = document
            .get_dictionary(page)
            .expect("page dictionary")
            .get(b"Contents")
            .expect("contents")
            .as_reference()
            .expect("contents reference");
        document
            .get_object_mut(contents_id)
            .expect("contents object")
            .as_stream_mut()
            .expect("contents stream")
            .set_plain_content(b"/Fm1 Do 1 0 0 1 20 0 cm /Fm1 Do".to_vec());

        let content = read_located_content(&document, page).expect("readable forms");

        assert_eq!(content.text_runs.len(), 2);
        assert_ne!(
            content.text_runs[0].form_path[0].operation_span,
            content.text_runs[1].form_path[0].operation_span
        );
        assert!(close(content.text_runs[0].run.bbox.x, 0.0));
        assert!(close(content.text_runs[1].run.bbox.x, 20.0));
    }

    #[test]
    fn a_cyclic_form_graph_stops_at_the_active_form() {
        use lopdf::{dictionary, Stream};

        let (mut document, page) = fixture::document_with_content(b"/A Do", Dictionary::new());
        let a = document.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Form" },
            b"/B Do".to_vec(),
        ));
        let b = document.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Form" },
            b"/A Do".to_vec(),
        ));
        document
            .get_object_mut(a)
            .expect("form A")
            .as_stream_mut()
            .expect("form A stream")
            .dict
            .set(
                "Resources",
                dictionary! { "XObject" => dictionary! { "B" => b } },
            );
        document
            .get_object_mut(b)
            .expect("form B")
            .as_stream_mut()
            .expect("form B stream")
            .dict
            .set(
                "Resources",
                dictionary! { "XObject" => dictionary! { "A" => a } },
            );
        document
            .get_dictionary_mut(page)
            .expect("page dictionary")
            .set(
                "Resources",
                dictionary! { "XObject" => dictionary! { "A" => a } },
            );

        let content = read_located_content(&document, page).expect("cycle is bounded");

        assert!(content.text_runs.is_empty());
    }

    /// A page that calls a chain of `levels` distinct forms, each invoking
    /// the next; only the innermost one shows text. The objects are all
    /// distinct, so the cycle guard never fires — depth is the only thing
    /// that can bound this descent.
    fn form_chain(levels: usize) -> (Document, ObjectId) {
        use lopdf::{dictionary, Stream};

        let (mut document, page) = fixture::document_with_content(b"/Fm Do", Dictionary::new());
        let forms: Vec<ObjectId> = (0..levels)
            .map(|level| {
                let content: &[u8] = if level + 1 == levels {
                    b"BT /F1 12 Tf (deep) Tj ET"
                } else {
                    b"/Fm Do"
                };
                document.add_object(Stream::new(
                    dictionary! { "Type" => "XObject", "Subtype" => "Form" },
                    content.to_vec(),
                ))
            })
            .collect();

        for (level, form) in forms.iter().enumerate() {
            let resources = if level + 1 == levels {
                fixture::helvetica_resources()
            } else {
                dictionary! { "XObject" => dictionary! { "Fm" => forms[level + 1] } }
            };
            document
                .get_object_mut(*form)
                .expect("form object")
                .as_stream_mut()
                .expect("form stream")
                .dict
                .set("Resources", resources);
        }

        document
            .get_dictionary_mut(page)
            .expect("page dictionary")
            .set(
                "Resources",
                dictionary! { "XObject" => dictionary! { "Fm" => forms[0] } },
            );

        (document, page)
    }

    #[test]
    fn a_form_chain_at_the_depth_cap_still_reads() {
        let (document, page) = form_chain(MAX_FORM_DEPTH);

        let content = read_located_content(&document, page).expect("the cap is inclusive");

        assert_eq!(content.text_runs.len(), 1);
        assert_eq!(content.text_runs[0].run.text, "deep");
        assert_eq!(content.text_runs[0].form_path.len(), MAX_FORM_DEPTH);
    }

    #[test]
    fn a_form_chain_past_the_depth_cap_is_refused() {
        let (document, page) = form_chain(MAX_FORM_DEPTH + 1);

        let error = read_located_content(&document, page).expect_err("one level too deep");

        assert!(
            matches!(error, EditError::FormNestingTooDeep { limit } if limit == MAX_FORM_DEPTH),
            "a stack overflow is an abort, not an error, so the descent refuses first: {error}"
        );
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
