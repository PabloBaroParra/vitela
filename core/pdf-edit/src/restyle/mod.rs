//! Bold, italic, underline and strikethrough toggled on an existing run.
//!
//! A run's look is the user's to change, but the method is this crate's:
//!
//! - **A real face first.** When the run's scope already has a font of the
//!   same family with the wanted weight and slant ([`sibling`]) — and that
//!   font can show every character of the run — the show operation is
//!   rewritten in it, between a `Tf` that selects it and one that restores
//!   the run's own font, as a retype's fallback stretches are.
//! - **Synthesized otherwise** ([`synthetic`]): bold as a stroked outline in
//!   the fill colour, italic as a 12° shear of the text matrix around the
//!   run's origin. Both are set back right after the run, so nothing else on
//!   the page changes.
//! - **Underline and strikethrough** are tagged lines drawn after the page's
//!   content ([`decoration`]), which is how they are found again: read back
//!   as part of the run's look, removed by the off-toggle, never drawn twice.
//!
//! Turning a style **off** reverses whichever method is on the page: a
//! stroked bold is set back to fill, a shear straightened, a bold or italic
//! face switched to its plain sibling. A run painted in a bold face whose
//! page has no regular sibling cannot be made regular — faking it is not
//! attempted — and [`text_run_look`] says so before the reader asks, through
//! [`RunLook::can_remove_bold`]. The same holds for italic. A line the file
//! drew on its own is not this crate's to find: it is neither reported nor
//! removed.

pub(crate) mod decoration;
pub(crate) mod sibling;
mod synthetic;

#[cfg(test)]
mod decoration_tests;
#[cfg(test)]
mod tests;

use lopdf::{Dictionary, Document, ObjectId};
use pdf_document::{TextRun, TextRunLook};

use crate::edit::{
    format_number, literal_string, matrix_operator, own_scope, resolve_text_run, scope_resources,
    scope_stream, splice,
};
use crate::encoding::fallback::Look;
use crate::error::EditError;
use crate::parse::interpreter::{LocatedContent, LocatedTextRun, PageStream};
use crate::parse::read_located_content;
use crate::style::PaintState;
use decoration::{Found, Line};
use sibling::Face;
use synthetic::{FILL_AND_STROKE, ITALIC_SHEAR, SHEAR_THRESHOLD};

/// How a run looks now, and which styles can be taken off it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLook {
    /// Bold and italic as a reader sees them: a bold or italic face, a
    /// stroked outline, a slanted matrix. Underline and strikethrough are the
    /// tagged lines this crate drew under the run ([`decoration`]).
    pub look: TextRunLook,
    /// Whether a bold run can be made regular: it is stroked, or its page
    /// has a regular sibling of its face that shows its text. `true` for a
    /// run that is not bold.
    pub can_remove_bold: bool,
    /// The same, for italic.
    pub can_remove_italic: bool,
}

/// How `run` looks on the page `page_object`, located as
/// [`crate::text_run_style`] locates it.
pub fn text_run_look(
    document: &Document,
    page_object: ObjectId,
    run: &TextRun,
) -> Result<RunLook, EditError> {
    let content = read_located_content(document, page_object)?;
    // Matched the way `crate::text_run_style` matches: a run with a retype
    // still pending shows text the file does not hold yet, so failing an
    // exact match it is the run with the same id painted in the same font.
    let located = match resolve_text_run(&content, run) {
        Ok(found) => found,
        Err(refusal) => content
            .text_runs
            .iter()
            .find(|candidate| {
                candidate.run.id == run.id
                    && candidate.run.resource_font_name == run.resource_font_name
            })
            .ok_or(refusal)?,
    };
    let resources = scope_resources(document, page_object, &located.form_path);
    let current = Current::read(document, &resources, &content, located);
    let look = current.look();
    let possible = |to: TextRunLook| plan(&current, look, to).is_ok();
    Ok(RunLook {
        look,
        can_remove_bold: !look.bold
            || possible(TextRunLook {
                bold: false,
                ..look
            }),
        can_remove_italic: !look.italic
            || possible(TextRunLook {
                italic: false,
                ..look
            }),
    })
}

/// Restyles `item` on `page_object` from `from` to `to`.
///
/// Every style is compared with what the page paints now, not with `from`
/// (the look the reader saw when recording, kept for undo): a restyle whose
/// styles already hold writes nothing for them, so a line is never drawn
/// twice, and a tagged line `to` drops is taken off the page.
///
/// Nothing is written unless everything can be: the rewrite and the lines
/// are built on a copy that replaces `document` only at the end.
pub fn restyle_text_run(
    document: &mut Document,
    page_object: ObjectId,
    item: &TextRun,
    _from: TextRunLook,
    to: TextRunLook,
) -> Result<(), EditError> {
    let located = read_located_content(document, page_object)?;
    let target = resolve_text_run(&located, item)?;
    let stream = scope_stream(document, &located, &target.form_path, target.stream_index)?;
    let resources = scope_resources(document, page_object, &target.form_path);
    let current = Current::read(document, &resources, &located, target);
    let plan = plan(&current, current.look(), to)?;

    let mut working = document.clone();
    let advance = plan.advance(target, &current.paint);
    if plan.rewrites() {
        let replacement = rewrite(target, &stream, &current, &plan)?;
        let scope = own_scope(&mut working, page_object, &target.form_path, &stream)?;
        splice(
            &mut working,
            scope.stream_object,
            &stream,
            target.operation_span.clone(),
            &replacement,
        )?;
    }
    let dropped: Vec<&Found> = current
        .lines
        .iter()
        .filter(|found| !wants(to, found.line))
        .collect();
    if !dropped.is_empty() {
        decoration::rewrite(&mut working, page_object, &located, &dropped, None)?;
    }
    let added: Vec<Line> = Line::ALL
        .into_iter()
        .filter(|line| wants(to, *line) && !current.has(*line))
        .collect();
    if !added.is_empty() {
        let placement = decoration::Placement {
            correction: crate::insert::correction_matrix(&working, page_object)?,
            ctm: target.placement.ctm,
            text_matrix: target.placement.text_matrix,
            font_size: target.placement.font_size,
            advance,
            fill_rgb: current.paint.fill_rgb,
        };
        let operators: Vec<u8> = added
            .into_iter()
            .flat_map(|line| decoration::operators(line, &placement))
            .collect();
        crate::insert::append_to_content(&mut working, page_object, &operators)?;
    }
    *document = working;
    Ok(())
}

/// Whether `look` asks for `line`.
fn wants(look: TextRunLook, line: Line) -> bool {
    match line {
        Line::Underline => look.underline,
        Line::Strike => look.strike,
    }
}

/// What the page paints for a run, and the faces it could switch to.
struct Current {
    /// The run's own face, by its font.
    face: Look,
    /// `Tr` where the run paints; `2` is a stroked (fake) bold.
    paint: PaintState,
    /// How far the run's text matrix leans.
    shear: f64,
    siblings: Vec<Face>,
    /// The tagged lines under / through the run.
    lines: Vec<Found>,
}

impl Current {
    fn read(
        document: &Document,
        resources: &Dictionary,
        content: &LocatedContent,
        located: &LocatedTextRun,
    ) -> Self {
        let own = &located.run.resource_font_name;
        let (face, siblings) = match sibling::own_face(document, resources, own) {
            Some((font, look)) => (
                look,
                sibling::siblings(document, resources, own, &font, &located.run.text),
            ),
            None => (Look::default(), Vec::new()),
        };
        Current {
            face,
            paint: PaintState::read(&located.paint_state),
            shear: synthetic::shear_of(located.placement.text_matrix),
            siblings,
            lines: decoration::of_run(&decoration::find(content), located)
                .into_iter()
                .cloned()
                .collect(),
        }
    }

    fn has(&self, line: Line) -> bool {
        self.lines.iter().any(|found| found.line == line)
    }

    fn stroked(&self) -> bool {
        self.paint.render_mode == FILL_AND_STROKE
    }

    fn slanted(&self) -> bool {
        self.shear.abs() > SHEAR_THRESHOLD
    }

    fn look(&self) -> TextRunLook {
        TextRunLook {
            bold: self.face.bold || self.stroked(),
            italic: self.face.italic || self.slanted(),
            underline: self.has(Line::Underline),
            strike: self.has(Line::Strike),
        }
    }
}

/// What a restyle changes about the show operation.
struct Plan<'a> {
    /// The sibling to paint in, when not the run's own font.
    face: Option<&'a Face>,
    /// `Some(true)`: stroke the outline; `Some(false)`: stop stroking it.
    stroke: Option<bool>,
    /// The lean to give the text matrix, when it changes.
    shear: Option<f64>,
}

impl Plan<'_> {
    fn rewrites(&self) -> bool {
        self.face.is_some() || self.stroke.is_some() || self.shear.is_some()
    }

    /// The run's advance once written: its own, or the sibling's for the
    /// same text, per PDF 32000-1 9.4.4 (kerning goes with the old glyphs).
    fn advance(&self, target: &LocatedTextRun, paint: &PaintState) -> f64 {
        let Some(face) = self.face else {
            return target.placement.advance;
        };
        let size = target.placement.font_size;
        let spaces = face.codes.iter().filter(|&&code| code == b' ').count() as f64;
        (face.info.width_of(&face.codes) * size
            + paint.char_spacing * face.codes.len() as f64
            + paint.word_spacing * spaces)
            * paint.horizontal_scale
    }
}

/// How to take the run from what it is (`now`) to `to`, or why it cannot be.
fn plan<'a>(
    current: &'a Current,
    now: TextRunLook,
    to: TextRunLook,
) -> Result<Plan<'a>, EditError> {
    if now.bold == to.bold && now.italic == to.italic {
        return Ok(Plan {
            face: None,
            stroke: None,
            shear: None,
        });
    }
    let face = choose_face(current, to)?;
    let look = face.map_or(current.face, |face| face.look);

    let wants_stroke = to.bold && !look.bold;
    let stroke = match (wants_stroke, current.stroked()) {
        (true, false) => {
            if current.paint.render_mode != 0 {
                return Err(EditError::TextRunNotRestylable {
                    reason: format!(
                        "its glyphs are painted in render mode {}, which a stroked bold \
                         would change",
                        current.paint.render_mode
                    ),
                });
            }
            Some(true)
        }
        (false, true) => Some(false),
        _ => None,
    };
    let wants_slant = to.italic && !look.italic;
    let shear = match (wants_slant, current.slanted()) {
        (true, false) => Some(ITALIC_SHEAR),
        (false, true) => Some(0.0),
        _ => None,
    };
    Ok(Plan {
        face,
        stroke,
        shear,
    })
}

/// The face that comes closest to `to` without having to fake a style
/// *off*: a regular face for a run that must not be bold, an upright one for
/// a run that must not be italic. The run's own font wins a tie.
fn choose_face(current: &Current, to: TextRunLook) -> Result<Option<&Face>, EditError> {
    let allowed = |look: Look| (to.bold || !look.bold) && (to.italic || !look.italic);
    let score = |look: Look| u8::from(look.bold == to.bold) + u8::from(look.italic == to.italic);

    let own = allowed(current.face).then(|| (score(current.face), None));
    let best = current
        .siblings
        .iter()
        .filter(|face| allowed(face.look))
        .map(|face| (score(face.look), Some(face)))
        .chain(own)
        .max_by_key(|(score, face)| (*score, face.is_none()));
    match best {
        Some((_, face)) => Ok(face),
        None if !to.bold && current.face.bold => {
            Err(EditError::StyleNotRemovable { style: "bold" })
        }
        None => Err(EditError::StyleNotRemovable { style: "italic" }),
    }
}

/// The bytes that replace the run's show operation.
fn rewrite(
    target: &LocatedTextRun,
    stream: &PageStream,
    current: &Current,
    plan: &Plan<'_>,
) -> Result<Vec<u8>, EditError> {
    let placement = &target.placement;
    let operation = &stream.bytes[target.operation_span.clone()];
    let operand_start = target.operand_span.start - target.operation_span.start;
    let operand_end = target.operand_span.end - target.operation_span.start;
    let operand: Vec<u8> = match plan.face {
        Some(face) if target.operator == "TJ" => {
            let mut array = vec![b'['];
            array.extend_from_slice(&literal_string(&face.codes));
            array.push(b']');
            array
        }
        Some(face) => literal_string(&face.codes),
        None => operation[operand_start..operand_end].to_vec(),
    };
    let size = format_number(placement.font_size);
    let mut out: Vec<u8> = Vec::new();

    match plan.stroke {
        Some(true) => {
            let width = synthetic::BOLD_STROKE
                * placement.font_size.abs()
                * placement.text_matrix.c.hypot(placement.text_matrix.d);
            out.extend_from_slice(&synthetic::bold_on(&target.paint_state, width));
        }
        Some(false) => out.extend_from_slice(b"0 Tr "),
        None => {}
    }
    if let Some(face) = plan.face {
        out.extend_from_slice(format!("/{} {size} Tf ", face.name).as_bytes());
    }
    match plan.shear {
        Some(shear) => {
            // The run is placed outright, so `'` must not feed a line first.
            if target.operator == "\"" {
                return Err(EditError::TextRunNotRestylable {
                    reason: "it is painted with the \" operator, which also sets the spacing \
                             for the text after it"
                        .to_string(),
                });
            }
            let leaning = synthetic::with_shear(placement.text_matrix, shear);
            out.extend_from_slice(&matrix_operator(leaning));
            out.extend_from_slice(&operand);
            let show = if target.operator == "TJ" { "TJ" } else { "Tj" };
            out.extend_from_slice(format!(" {show} ").as_bytes());
        }
        None => {
            out.extend_from_slice(&operation[..operand_start]);
            out.extend_from_slice(&operand);
            out.extend_from_slice(&operation[operand_end..]);
            out.push(b' ');
        }
    }
    if plan.face.is_some() {
        out.extend_from_slice(format!("/{} {size} Tf ", target.run.resource_font_name).as_bytes());
    }
    if plan.shear.is_some() {
        out.extend_from_slice(&restore_text_matrix(
            target,
            plan.advance(target, &current.paint),
        )?);
    }
    match plan.stroke {
        Some(true) => out.extend_from_slice(&synthetic::bold_restore(
            &target.paint_state,
            current.paint.render_mode,
        )),
        Some(false) => out.extend_from_slice(format!("{FILL_AND_STROKE} Tr").as_bytes()),
        None => {}
    }
    while out.last() == Some(&b' ') {
        out.pop();
    }
    Ok(out)
}

/// Puts the text matrix back where showing the run upright would have left
/// it: its line matrix, then a `TJ` displacement of everything the line had
/// advanced by — the same restore a moved run inside a form makes.
fn restore_text_matrix(target: &LocatedTextRun, advance: f64) -> Result<Vec<u8>, EditError> {
    let placement = &target.placement;
    let inverse_line =
        placement
            .line_matrix
            .invert()
            .ok_or_else(|| EditError::MalformedContent {
                reason: "text is placed by a line matrix that collapses to zero area".to_string(),
                offset: target.operation_span.start,
            })?;
    let carried = placement.text_matrix.then(inverse_line).e;
    let mut out = matrix_operator(placement.line_matrix);
    if placement.advance_scale.abs() >= 1e-9 {
        let adjustment = -(carried + advance) * 1000.0 / placement.advance_scale;
        if adjustment.abs() >= 1e-9 {
            out.extend_from_slice(format!("[{}] TJ ", format_number(adjustment)).as_bytes());
        }
    }
    Ok(out)
}
