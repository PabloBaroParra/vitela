//! Rewrites of vector paths (B25 P1, T-206): move, resize, remove.
//!
//! Same posture as the image rewrites in [`crate::edit`]: the command's item
//! is re-resolved against the page as it stands, one span is spliced, and
//! every other byte stays where it was. What differs is the mechanism.
//!
//! An image is placed by a `cm`, so moving one is a new `cm`. A path is not
//! placed by anything — its coordinates *are* its place — so moving or
//! resizing one rewrites those coordinates. That is also the only way a
//! resize can leave the **line width alone** (decided by the user
//! 2026-10-28): a scaling `cm` would scale the stroke with the shape, and a
//! table frame stretched to twice its width would come out with borders
//! twice as thick.

use crate::edit::{own_scope, resolve, same_rect, scope_stream, splice};
use crate::error::EditError;
use crate::parse::{path, read_located_content, LocatedContent, LocatedPath};
use lopdf::{Document, ObjectId};
use pdf_document::{PathItem, Rect};

/// Moves a path so its box sits at `to`'s origin. Only the origin is read;
/// the path keeps its size.
pub fn move_path(
    document: &mut Document,
    page_object: ObjectId,
    item: &PathItem,
    to: Rect,
) -> Result<(), EditError> {
    let to = Rect {
        x: to.x,
        y: to.y,
        ..item.bbox
    };
    reshape_path(document, page_object, item, to)
}

/// Reshapes a path so its box becomes `to`, keeping its line width.
///
/// An axis the path has no extent along — a horizontal rule's height — is
/// moved to `to` but not stretched: there is nothing to scale, and a rule
/// asked to become a box is still a rule.
pub fn resize_path(
    document: &mut Document,
    page_object: ObjectId,
    item: &PathItem,
    to: Rect,
) -> Result<(), EditError> {
    reshape_path(document, page_object, item, to)
}

/// Deletes a path's construction and painting operators. The graphics state
/// around it — colour, line width, a `q`/`Q` pair — is left as it was, since
/// whatever follows may still rely on it.
pub fn remove_path(
    document: &mut Document,
    page_object: ObjectId,
    item: &PathItem,
) -> Result<(), EditError> {
    let located = read_located_content(document, page_object)?;
    let target = resolve_path(&located, item)?;
    let stream = scope_stream(document, &located, &target.form_path, target.stream_index)?;

    let span = target.operation_span.clone();
    let scope = own_scope(document, page_object, &target.form_path, &stream)?;
    splice(document, scope.stream_object, &stream, span, &[])
}

fn reshape_path(
    document: &mut Document,
    page_object: ObjectId,
    item: &PathItem,
    to: Rect,
) -> Result<(), EditError> {
    let located = read_located_content(document, page_object)?;
    let target = resolve_path(&located, item)?;
    let stream = scope_stream(document, &located, &target.form_path, target.stream_index)?;

    let inverse = target
        .ctm
        .invert()
        .ok_or_else(|| EditError::MalformedContent {
            reason: "path is drawn under a matrix that collapses to zero area".to_string(),
            offset: target.operation_span.start,
        })?;
    // Page space is where the user dragged; the segments are in user space.
    // So: up through the CTM, across by the page-space mapping, back down.
    let user_map = target
        .ctm
        .then(path::page_mapping(target.item.bbox, to))
        .then(inverse);
    let replacement = path::rewrite(&target.segments, user_map, &target.paint_operator);

    let span = target.operation_span.clone();
    let scope = own_scope(document, page_object, &target.form_path, &stream)?;
    splice(document, scope.stream_object, &stream, span, &replacement)
}

/// A path's identity is how it is painted and where. Two identical rules at
/// the same spot are the ambiguity [`resolve`] refuses rather than guesses.
fn resolve_path<'a>(
    located: &'a LocatedContent,
    item: &PathItem,
) -> Result<&'a LocatedPath, EditError> {
    resolve(
        &located.paths,
        item.id,
        |candidate| candidate.item.id,
        |candidate| candidate.item.paint == item.paint && same_rect(candidate.item.bbox, item.bbox),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;
    use crate::parse::read_page_object_content;
    use pdf_document::{PageId, PathPaint};

    fn page(content: &[u8]) -> (Document, ObjectId) {
        fixture::document_with_content(content, fixture::helvetica_resources())
    }

    fn paths(document: &Document, page_object: ObjectId) -> Vec<PathItem> {
        read_page_object_content(document, page_object, PageId(0))
            .expect("readable page")
            .paths
    }

    fn content(document: &Document, page_object: ObjectId) -> String {
        let located = read_located_content(document, page_object).expect("readable page");
        String::from_utf8(located.streams[0].bytes.clone()).expect("ascii content")
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn close(left: Rect, right: Rect) -> bool {
        same_rect(left, right)
    }

    /// The client box of the reference quote: one `re` stroked.
    #[test]
    fn a_stroked_rectangle_is_a_path_item() {
        let (document, page_object) = page(b"0.5 w 319.05 135.5 251 125 re S");

        let found = paths(&document, page_object);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].paint, PathPaint::Stroke);
        assert!(close(found[0].bbox, rect(319.05, 135.5, 251.0, 125.0)));
    }

    #[test]
    fn a_clip_is_not_an_item() {
        let (document, page_object) = page(b"q 0 0 100 100 re W n 10 10 5 5 re f Q");

        let found = paths(&document, page_object);

        assert_eq!(found.len(), 1, "only the filled square paints");
        assert!(close(found[0].bbox, rect(10.0, 10.0, 5.0, 5.0)));
    }

    #[test]
    fn a_path_that_clips_and_paints_is_not_an_item() {
        let (document, page_object) = page(b"0 0 100 100 re W f");

        assert!(paths(&document, page_object).is_empty());
    }

    #[test]
    fn the_ctm_places_the_path_box() {
        let (document, page_object) = page(b"q 2 0 0 2 10 20 cm 0 0 m 5 0 l S Q");

        let found = paths(&document, page_object);

        assert!(close(found[0].bbox, rect(10.0, 20.0, 10.0, 0.0)));
    }

    #[test]
    fn text_and_paths_are_read_side_by_side() {
        let (document, page_object) =
            page(b"0 0 10 10 re f BT /F1 12 Tf 100 700 Td (Hi) Tj ET 0 0 m 1 1 l S");

        let content = read_page_object_content(&document, page_object, PageId(0)).unwrap();

        assert_eq!(content.text_runs.len(), 1);
        assert_eq!(content.paths.len(), 2);
        assert_eq!(content.paths[1].id.0, 1);
    }

    #[test]
    fn moving_a_path_rewrites_its_coordinates_and_nothing_else() {
        let (mut document, page_object) = page(b"0.5 w 10 20 30 40 re S BT /F1 12 Tf (x) Tj ET");
        let item = paths(&document, page_object).remove(0);

        move_path(
            &mut document,
            page_object,
            &item,
            rect(110.0, 220.0, 0.0, 0.0),
        )
        .unwrap();

        assert_eq!(
            content(&document, page_object),
            "0.5 w 110 220 30 40 re\nS BT /F1 12 Tf (x) Tj ET"
        );
        assert!(close(
            paths(&document, page_object)[0].bbox,
            rect(110.0, 220.0, 30.0, 40.0)
        ));
    }

    /// The user's decision: stretching a frame changes the frame, not the
    /// pen. The `w` before it is left exactly as written.
    #[test]
    fn resizing_a_path_keeps_its_line_width() {
        let (mut document, page_object) = page(b"2 w 10 20 30 40 re S");
        let item = paths(&document, page_object).remove(0);

        resize_path(
            &mut document,
            page_object,
            &item,
            rect(10.0, 20.0, 60.0, 80.0),
        )
        .unwrap();

        assert_eq!(content(&document, page_object), "2 w 10 20 60 80 re\nS");
    }

    #[test]
    fn resizing_under_a_scaled_ctm_lands_in_page_space() {
        let (mut document, page_object) = page(b"q 2 0 0 2 0 0 cm 5 5 10 10 re f Q");
        let item = paths(&document, page_object).remove(0);
        assert!(close(item.bbox, rect(10.0, 10.0, 20.0, 20.0)));

        resize_path(
            &mut document,
            page_object,
            &item,
            rect(0.0, 0.0, 40.0, 40.0),
        )
        .unwrap();

        assert!(close(
            paths(&document, page_object)[0].bbox,
            rect(0.0, 0.0, 40.0, 40.0)
        ));
        assert!(content(&document, page_object).starts_with("q 2 0 0 2 0 0 cm "));
    }

    #[test]
    fn a_curve_moves_with_every_control_point() {
        let (mut document, page_object) = page(b"0 0 m 10 10 20 10 30 0 c 40 5 50 0 v S");
        let item = paths(&document, page_object).remove(0);

        move_path(&mut document, page_object, &item, rect(1.0, 2.0, 0.0, 0.0)).unwrap();

        assert_eq!(
            content(&document, page_object),
            "1 2 m\n11 12 21 12 31 2 c\n41 7 51 2 v\nS"
        );
    }

    #[test]
    fn removing_a_path_leaves_the_graphics_state_around_it() {
        let (mut document, page_object) = page(b"q 1 0 0 RG 10 20 30 40 re S Q 0 0 5 5 re f");
        let item = paths(&document, page_object).remove(0);

        remove_path(&mut document, page_object, &item).unwrap();

        assert_eq!(
            content(&document, page_object),
            "q 1 0 0 RG  Q 0 0 5 5 re f"
        );
        assert_eq!(paths(&document, page_object).len(), 1);
    }

    #[test]
    fn a_path_that_is_no_longer_there_is_not_found() {
        let (mut document, page_object) = page(b"10 20 30 40 re S");
        let mut item = paths(&document, page_object).remove(0);
        item.bbox.x += 5.0;

        assert!(matches!(
            remove_path(&mut document, page_object, &item),
            Err(EditError::ItemNotFound(_))
        ));
    }
}
