//! The annotation edits a boundary records, as `pdf_document::Command`s —
//! the rules every boundary needs between "the user asked for this" and
//! "this joins the edit log".
//!
//! This used to live only in `pdf-ffi` (`DocumentState::build_core_command`),
//! so a boundary that does not cross UniFFI (`pdf-wasm`) would have had to
//! copy it (T-235). What stays at a boundary is its own vocabulary: turning
//! a page *position* into a [`PageId`], and its DTOs into the core types
//! these functions take.
//!
//! - **Identity** — [`AnnotationIdCursor`]: one id per new annotation, never
//!   reused for the life of the session, not even after an undo takes the
//!   annotation back out. It starts at zero because an opened document's
//!   `AnnotationSet` always starts empty (`pdf_save::document_from_lopdf`):
//!   the file's own annotations stay in its object graph.
//! - **Construction** — [`add_annotation`], [`change_annotation`],
//!   [`remove_annotation`]: each one resolves what the command has to carry
//!   (the removed annotation itself, the `before` half of a replacement)
//!   from the current set, so undo never has to reconstruct anything.
//! - **The no-op retype** — [`change_annotation`] answers `None` when
//!   retyping a box to the text it already has: that edited nothing, and must
//!   not become an undo step the user has to step over.

use pdf_document::{Annotation, AnnotationId, AnnotationSet, Color, Command, PageId, Rect};

use crate::AnnotateError;

/// Hands out the [`AnnotationId`] of each annotation a session adds.
///
/// A plain counter, not "one past the highest id in the set": an undo takes
/// an annotation out of the set, and a redo puts it back under the id it
/// had, so an id the set no longer holds is still spent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnnotationIdCursor {
    next: u64,
}

impl AnnotationIdCursor {
    /// A cursor for a session that has added nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The next id, spent from now on.
    pub fn allocate(&mut self) -> AnnotationId {
        let id = AnnotationId(self.next);
        self.next += 1;
        id
    }
}

/// An annotation a boundary asks to add, in core types. Which page it goes
/// on is [`add_annotation`]'s other argument.
#[derive(Debug, Clone, PartialEq)]
pub enum NewAnnotation {
    Highlight {
        rect: Rect,
        color: Color,
    },
    Underline {
        rect: Rect,
        color: Color,
    },
    Strikeout {
        rect: Rect,
        color: Color,
    },
    Shape {
        rect: Rect,
        color: Color,
    },
    Ink {
        points: Vec<(f64, f64)>,
        color: Color,
    },
    TextNote {
        rect: Rect,
        contents: String,
    },
    FreeText {
        rect: Rect,
        contents: String,
    },
    /// An image stamp from PNG/JPEG bytes.
    Stamp {
        image: Vec<u8>,
        rect: Rect,
    },
}

/// The `AddAnnotation` command for `new` on `page`.
///
/// The id is spent before the annotation is built, so a refused free-text
/// box or undecodable stamp still uses one up. Harmless — ids only have to
/// be unique — and exactly what every boundary did before this moved here.
pub fn add_annotation(
    ids: &mut AnnotationIdCursor,
    page: PageId,
    new: NewAnnotation,
) -> Result<Command, AnnotateError> {
    let id = ids.allocate();
    let annotation = match new {
        NewAnnotation::Highlight { rect, color } => crate::highlight(id, page, rect, color),
        NewAnnotation::Underline { rect, color } => crate::underline(id, page, rect, color),
        NewAnnotation::Strikeout { rect, color } => crate::strikeout(id, page, rect, color),
        NewAnnotation::Shape { rect, color } => crate::shape(id, page, rect, color),
        NewAnnotation::Ink { points, color } => crate::ink(id, page, points, color),
        NewAnnotation::TextNote { rect, contents } => crate::text_note(id, page, rect, contents),
        NewAnnotation::FreeText { rect, contents } => crate::free_text(id, page, rect, &contents)?,
        NewAnnotation::Stamp { image, rect } => {
            crate::stamp_from_image_bytes(id, page, &image, rect)?
        }
    };
    Ok(Command::AddAnnotation(annotation))
}

/// A change to an annotation the session already holds.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationChange {
    /// Retype a free-text box.
    SetContents(String),
    Move {
        dx: f64,
        dy: f64,
    },
    Resize(Rect),
    Restyle(Color),
}

/// The `ReplaceAnnotation` command that applies `change` to annotation `id`,
/// or `None` when it is a retype that leaves the text as it was.
///
/// Only a retype is answered `None`. A move by zero is recorded like any
/// other move — what boundaries have always done, so it stays that way.
pub fn change_annotation(
    annotations: &AnnotationSet,
    id: AnnotationId,
    change: AnnotationChange,
) -> Result<Option<Command>, AnnotateError> {
    let before = existing(annotations, id)?;
    let mut after = before.clone();
    let is_retype = matches!(change, AnnotationChange::SetContents(_));
    match change {
        AnnotationChange::SetContents(contents) => {
            crate::set_annotation_contents(&mut after, &contents)?
        }
        AnnotationChange::Move { dx, dy } => crate::move_annotation(&mut after, dx, dy)?,
        AnnotationChange::Resize(rect) => crate::resize_annotation(&mut after, rect)?,
        AnnotationChange::Restyle(color) => crate::restyle_annotation(&mut after, color)?,
    }
    if is_retype && before == after {
        return Ok(None);
    }
    Ok(Some(Command::ReplaceAnnotation { before, after }))
}

/// The `RemoveAnnotation` command for annotation `id`, carrying the removed
/// annotation itself so undo can put it back.
pub fn remove_annotation(
    annotations: &AnnotationSet,
    id: AnnotationId,
) -> Result<Command, AnnotateError> {
    Ok(Command::RemoveAnnotation(existing(annotations, id)?))
}

/// A copy of annotation `id`, or the error that names it as missing.
fn existing(annotations: &AnnotationSet, id: AnnotationId) -> Result<Annotation, AnnotateError> {
    annotations
        .get(id)
        .cloned()
        .ok_or(AnnotateError::AnnotationNotFound(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::AnnotationKind;

    fn rect() -> Rect {
        Rect {
            x: 10.0,
            y: 20.0,
            width: 120.0,
            height: 40.0,
        }
    }

    fn color() -> Color {
        Color { r: 255, g: 0, b: 0 }
    }

    fn added(command: Command) -> Annotation {
        match command {
            Command::AddAnnotation(annotation) => annotation,
            other => panic!("expected AddAnnotation, got {other:?}"),
        }
    }

    fn a_set_with(annotation: Annotation) -> AnnotationSet {
        let mut set = AnnotationSet::new();
        set.insert(annotation);
        set
    }

    fn a_png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbImage::new(2, 1)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("encodes");
        bytes
    }

    #[test]
    fn the_cursor_starts_at_zero_and_never_repeats() {
        let mut ids = AnnotationIdCursor::new();

        assert_eq!(ids.allocate(), AnnotationId(0));
        assert_eq!(ids.allocate(), AnnotationId(1));
        assert_eq!(ids.allocate(), AnnotationId(2));
    }

    #[test]
    fn each_new_annotation_gets_the_next_id_and_its_page() {
        let mut ids = AnnotationIdCursor::new();

        let first = added(
            add_annotation(
                &mut ids,
                PageId(3),
                NewAnnotation::Highlight {
                    rect: rect(),
                    color: color(),
                },
            )
            .expect("builds"),
        );
        let second = added(
            add_annotation(
                &mut ids,
                PageId(5),
                NewAnnotation::TextNote {
                    rect: rect(),
                    contents: "Call back".to_string(),
                },
            )
            .expect("builds"),
        );

        assert_eq!((first.id, first.page), (AnnotationId(0), PageId(3)));
        assert!(matches!(first.kind, AnnotationKind::Highlight { .. }));
        assert_eq!((second.id, second.page), (AnnotationId(1), PageId(5)));
        assert!(
            matches!(second.kind, AnnotationKind::TextNote { ref contents, .. } if contents == "Call back")
        );
    }

    /// One case per kind, so a variant routed to the wrong builder shows.
    #[test]
    fn every_kind_builds_its_own_annotation() {
        let mut ids = AnnotationIdCursor::new();
        let cases = [
            NewAnnotation::Underline {
                rect: rect(),
                color: color(),
            },
            NewAnnotation::Strikeout {
                rect: rect(),
                color: color(),
            },
            NewAnnotation::Shape {
                rect: rect(),
                color: color(),
            },
            NewAnnotation::Ink {
                points: vec![(1.0, 2.0), (3.0, 4.0)],
                color: color(),
            },
            NewAnnotation::FreeText {
                rect: rect(),
                contents: "Hello".to_string(),
            },
            NewAnnotation::Stamp {
                image: a_png(),
                rect: rect(),
            },
        ];

        let kinds: Vec<_> = cases
            .into_iter()
            .map(|new| added(add_annotation(&mut ids, PageId(0), new).expect("builds")).kind)
            .collect();

        assert!(matches!(kinds[0], AnnotationKind::Underline { .. }));
        assert!(matches!(kinds[1], AnnotationKind::Strikeout { .. }));
        assert!(matches!(kinds[2], AnnotationKind::Shape { .. }));
        assert!(matches!(kinds[3], AnnotationKind::Ink { ref points, .. } if points.len() == 2));
        assert!(matches!(kinds[4], AnnotationKind::FreeText { .. }));
        assert!(matches!(kinds[5], AnnotationKind::Stamp { .. }));
    }

    #[test]
    fn a_refused_build_still_spends_its_id() {
        let mut ids = AnnotationIdCursor::new();

        let refused = add_annotation(
            &mut ids,
            PageId(0),
            NewAnnotation::FreeText {
                rect: rect(),
                contents: "日本".to_string(),
            },
        );

        assert!(matches!(refused, Err(AnnotateError::EncodingGap { .. })));
        assert_eq!(ids.allocate(), AnnotationId(1));
    }

    #[test]
    fn a_stamp_from_bytes_that_are_no_image_is_refused() {
        let refused = add_annotation(
            &mut AnnotationIdCursor::new(),
            PageId(0),
            NewAnnotation::Stamp {
                image: b"not an image".to_vec(),
                rect: rect(),
            },
        );

        assert!(matches!(refused, Err(AnnotateError::InvalidImage(_))));
    }

    #[test]
    fn a_change_carries_both_halves_of_the_annotation() {
        let before = crate::highlight(AnnotationId(4), PageId(0), rect(), color());
        let set = a_set_with(before.clone());

        let command = change_annotation(
            &set,
            AnnotationId(4),
            AnnotationChange::Move { dx: 5.0, dy: -5.0 },
        )
        .expect("moves")
        .expect("records a step");

        let Command::ReplaceAnnotation {
            before: from,
            after,
        } = command
        else {
            panic!("expected ReplaceAnnotation");
        };
        assert_eq!(from, before);
        assert!(
            matches!(after.kind, AnnotationKind::Highlight { rect: r, .. } if r.x == 15.0 && r.y == 15.0)
        );
    }

    #[test]
    fn resize_and_restyle_replace_the_annotation() {
        let set = a_set_with(crate::shape(AnnotationId(1), PageId(0), rect(), color()));
        let bigger = Rect {
            width: 200.0,
            ..rect()
        };
        let blue = Color { r: 0, g: 0, b: 255 };

        let resized = change_annotation(&set, AnnotationId(1), AnnotationChange::Resize(bigger))
            .expect("resizes")
            .expect("records a step");
        let restyled = change_annotation(&set, AnnotationId(1), AnnotationChange::Restyle(blue))
            .expect("restyles")
            .expect("records a step");

        assert!(
            matches!(resized, Command::ReplaceAnnotation { after, .. } if matches!(after.kind, AnnotationKind::Shape { rect, .. } if rect == bigger))
        );
        assert!(
            matches!(restyled, Command::ReplaceAnnotation { after, .. } if matches!(after.kind, AnnotationKind::Shape { color, .. } if color == blue))
        );
    }

    #[test]
    fn retyping_a_box_to_new_text_records_it() {
        let set = a_set_with(
            crate::free_text(AnnotationId(2), PageId(0), rect(), "old").expect("builds"),
        );

        let command = change_annotation(
            &set,
            AnnotationId(2),
            AnnotationChange::SetContents("new".to_string()),
        )
        .expect("retypes");

        assert!(
            matches!(command, Some(Command::ReplaceAnnotation { after, .. }) if matches!(after.kind, AnnotationKind::FreeText { ref contents, .. } if contents == "new"))
        );
    }

    /// The retype that edited nothing is not a step the user would have to
    /// undo past.
    #[test]
    fn retyping_a_box_to_the_text_it_has_records_nothing() {
        let set = a_set_with(
            crate::free_text(AnnotationId(2), PageId(0), rect(), "same").expect("builds"),
        );

        let command = change_annotation(
            &set,
            AnnotationId(2),
            AnnotationChange::SetContents("same".to_string()),
        )
        .expect("retypes");

        assert_eq!(command, None);
    }

    /// Only the retype is a no-op: a move by nothing is recorded, as it
    /// always has been.
    #[test]
    fn a_move_by_nothing_is_still_recorded() {
        let set = a_set_with(crate::highlight(
            AnnotationId(0),
            PageId(0),
            rect(),
            color(),
        ));

        let command = change_annotation(
            &set,
            AnnotationId(0),
            AnnotationChange::Move { dx: 0.0, dy: 0.0 },
        )
        .expect("moves");

        assert!(command.is_some());
    }

    #[test]
    fn a_change_the_kind_cannot_take_is_refused() {
        let set = a_set_with(crate::highlight(
            AnnotationId(0),
            PageId(0),
            rect(),
            color(),
        ));

        let refused = change_annotation(
            &set,
            AnnotationId(0),
            AnnotationChange::SetContents("text".to_string()),
        );

        assert!(matches!(
            refused,
            Err(AnnotateError::UnsupportedOperation(_))
        ));
    }

    #[test]
    fn a_removal_carries_the_removed_annotation() {
        let annotation = crate::highlight(AnnotationId(7), PageId(1), rect(), color());
        let set = a_set_with(annotation.clone());

        assert_eq!(
            remove_annotation(&set, AnnotationId(7)),
            Ok(Command::RemoveAnnotation(annotation))
        );
    }

    #[test]
    fn an_absent_annotation_is_reported_by_id() {
        let set = AnnotationSet::new();

        assert_eq!(
            remove_annotation(&set, AnnotationId(9)),
            Err(AnnotateError::AnnotationNotFound(AnnotationId(9)))
        );
        assert_eq!(
            change_annotation(&set, AnnotationId(9), AnnotationChange::Restyle(color())),
            Err(AnnotateError::AnnotationNotFound(AnnotationId(9)))
        );
    }
}
