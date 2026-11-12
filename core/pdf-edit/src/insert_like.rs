//! A new run painted the way an existing one is (paragraph text blocks).
//!
//! [`crate::insert_text_run`] paints new text plainly: the font resource it
//! names, the size its box gives, black, no spacing. That is right for text
//! the reader types somewhere new, and wrong for a line a retyped paragraph
//! grows — the paragraph's other lines keep their colour, character spacing
//! and horizontal scale, and the new one would stand out.
//!
//! So this copies the look of a **template** run already on the page — the
//! same operators [`crate::move_text_run`] uses to repaint a run brought to
//! the front — and only changes the text and where it lands.

use crate::edit::{format_number, literal_string, resolve_text_run};
use crate::encoding::resolve_font;
use crate::error::EditError;
use crate::insert::{append_to_content, correction_matrix, font_resource, matrix_operator};
use crate::parse::matrix::Matrix;
use crate::parse::read_located_content;
use lopdf::{Document, ObjectId};
use pdf_document::TextRun;

/// Appends `run` to the page, painted like `template`: the template's font
/// resource and size, its `gs`, fill colour, `Tc`, `Tw`, `Tz`, `Ts` and
/// `Tr`, its transform — moved by the page-space offset from the template's
/// box origin to `run.bbox`'s.
///
/// Only `run.text` and `run.bbox`'s origin are read from `run`. The text is
/// encoded against the template's font before anything is written; the
/// characters that font lacks are written in its standard-14 fallback face
/// (see [`crate::encoding::fallback`]), and one neither has fails with an
/// [`EditError::EncodingGap`] instead of painting the wrong glyphs.
///
/// A template inside a Form XObject is refused with
/// [`EditError::ContentNotCopyable`]: its font and `gs` names belong to the
/// form's resources, not to the page stream the run is appended to.
pub fn insert_text_run_like(
    document: &mut Document,
    page_object: ObjectId,
    run: &TextRun,
    template: &TextRun,
) -> Result<(), EditError> {
    let located = read_located_content(document, page_object)?;
    let target = resolve_text_run(&located, template)?;
    if !target.form_path.is_empty() {
        return Err(EditError::ContentNotCopyable {
            reason: "a run inside a form xobject cannot be used as a style template".to_string(),
        });
    }

    let font_name = &target.run.resource_font_name;
    let font_dict = font_resource(document, page_object, font_name).ok_or_else(|| {
        EditError::FontResourceMissing {
            resource_font_name: font_name.clone(),
        }
    })?;
    let font = resolve_font(document, &font_dict, font_name)?;
    let segments = font.encode_with_fallback(&run.text)?;
    let fallback_base = font.fallback_base_font();
    let fallback_name = segments.iter().any(|segment| segment.fallback).then(|| {
        crate::insert::fallback_font_resource_name_in(
            document,
            &crate::insert::owned_resources_snapshot(document, page_object),
            &font,
        )
    });

    let placement = target.placement;
    let inverse_ctm = placement
        .ctm
        .invert()
        .ok_or_else(|| EditError::MalformedContent {
            reason: "text is placed by a matrix that collapses to zero area".to_string(),
            offset: target.operation_span.start,
        })?;
    // The same pull-back `move_text_run` makes: a page-space shift composed
    // on the page-facing side of the template's text matrix.
    let shift = Matrix::translate(run.bbox.x - template.bbox.x, run.bbox.y - template.bbox.y);
    let text_matrix = placement
        .text_matrix
        .then(placement.ctm)
        .then(shift)
        .then(inverse_ctm);

    let correction = correction_matrix(document, page_object)?;
    let mut operators = format!(
        "\nq {} {} ",
        matrix_operator(correction),
        matrix_operator(placement.ctm)
    )
    .into_bytes();
    operators.extend_from_slice(&target.paint_state);
    operators.extend_from_slice(
        format!(
            "BT /{} {} Tf {} {} {} {} {} {} Tm ",
            font_name,
            format_number(placement.font_size),
            format_number(text_matrix.a),
            format_number(text_matrix.b),
            format_number(text_matrix.c),
            format_number(text_matrix.d),
            format_number(text_matrix.e),
            format_number(text_matrix.f),
        )
        .as_bytes(),
    );
    let size = format_number(placement.font_size);
    let mut current = font_name.as_str();
    for segment in &segments {
        let wanted = match (&fallback_name, segment.fallback) {
            (Some(fallback), true) => fallback.as_str(),
            _ => font_name.as_str(),
        };
        if wanted != current {
            operators.extend_from_slice(format!("/{wanted} {size} Tf ").as_bytes());
            current = wanted;
        }
        operators.extend_from_slice(&literal_string(&segment.codes));
        operators.extend_from_slice(b" Tj ");
    }
    // Back to the template's font, as every fallback write ends: a later
    // retype of a fallback stretch finds the font it belongs to there.
    if current != font_name {
        operators.extend_from_slice(format!("/{font_name} {size} Tf ").as_bytes());
    }
    operators.extend_from_slice(b"ET Q\n");

    if let Some(fallback) = &fallback_name {
        crate::insert::ensure_standard_font_resource(
            document,
            page_object,
            fallback,
            fallback_base,
        )?;
    }
    append_to_content(document, page_object, &operators)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{document_with_content, helvetica_resources};
    use pdf_document::PageId;

    const STYLED: &[u8] = b"BT /F1 12 Tf 0.2 0.4 0.6 rg 1.5 Tc 90 Tz 100 700 Td (Hello) Tj ET 0 g";

    fn located_runs(
        document: &Document,
        page: ObjectId,
    ) -> Vec<crate::parse::interpreter::LocatedTextRun> {
        read_located_content(document, page)
            .expect("readable page")
            .text_runs
    }

    fn template(document: &Document, page: ObjectId) -> TextRun {
        crate::read_page_object_content(document, page, PageId(0))
            .expect("readable page")
            .text_runs
            .remove(0)
    }

    #[test]
    fn the_new_run_keeps_the_templates_colour_and_spacing() {
        let (mut document, page) = document_with_content(STYLED, helvetica_resources());
        let template = template(&document, page);
        let run = TextRun {
            text: "World".to_string(),
            bbox: pdf_document::Rect {
                y: template.bbox.y - 14.0,
                ..template.bbox
            },
            ..template.clone()
        };

        insert_text_run_like(&mut document, page, &run, &template).expect("insertable");

        let runs = located_runs(&document, page);
        assert_eq!(runs.len(), 2);
        let (old, new) = (&runs[0], &runs[1]);
        assert_eq!(new.run.text, "World");
        assert_eq!(new.paint_state, old.paint_state);
        let paint = String::from_utf8_lossy(&new.paint_state).to_string();
        assert!(paint.contains("0.2 0.4 0.6 rg"), "{paint}");
        assert!(
            paint.contains("1.5 Tc") && paint.contains("90 Tz"),
            "{paint}"
        );
        assert_eq!(new.placement.font_size, 12.0);
        assert!((new.run.bbox.x - template.bbox.x).abs() < 1e-6);
        assert!((new.run.bbox.y - (template.bbox.y - 14.0)).abs() < 1e-6);
        assert!((new.run.bbox.height - template.bbox.height).abs() < 1e-6);
    }

    #[test]
    fn text_the_templates_font_cannot_show_is_refused_before_writing() {
        let (mut document, page) = document_with_content(STYLED, helvetica_resources());
        let template = template(&document, page);
        let run = TextRun {
            text: "日本".to_string(),
            ..template.clone()
        };

        let error = insert_text_run_like(&mut document, page, &run, &template).unwrap_err();

        assert!(matches!(error, EditError::EncodingGap { .. }), "{error:?}");
        assert_eq!(located_runs(&document, page).len(), 1);
    }

    #[test]
    fn a_template_the_page_does_not_paint_is_not_found() {
        let (mut document, page) = document_with_content(STYLED, helvetica_resources());
        let mut template = template(&document, page);
        template.text = "Gone".to_string();

        let error =
            insert_text_run_like(&mut document, page, &template.clone(), &template).unwrap_err();

        assert!(matches!(error, EditError::ItemNotFound(_)), "{error:?}");
    }
}
