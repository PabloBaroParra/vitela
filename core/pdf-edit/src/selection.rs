//! A set of page items edited as one (B25 P3, T-208): what a marquee or a
//! SHIFT+click gathers — text runs, images and vector paths of one page —
//! moved or removed as a single undo step.
//!
//! Like a text block ([`crate::group`]), a selection is not an object in the
//! file. Every edit of it is a [`Command::Batch`] of the ordinary edits of
//! its members, so the save replays it the way it replays them one by one.
//! A "table" the user drags is exactly this: its border paths plus the text
//! inside them, gathered by the marquee.

use pdf_document::{Command, ImageItem, PathItem, Rect, TextRun};

/// The members of a selection, each as `read_page_content` (and
/// `read_page_paths`) reads it now.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Selection {
    pub runs: Vec<TextRun>,
    pub images: Vec<ImageItem>,
    pub paths: Vec<PathItem>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty() && self.images.is_empty() && self.paths.is_empty()
    }

    /// Whether every member sits on one page — a move by an offset only
    /// means something there.
    pub fn is_on_one_page(&self) -> bool {
        let mut pages = self
            .runs
            .iter()
            .map(|run| run.page)
            .chain(self.images.iter().map(|image| image.page))
            .chain(self.paths.iter().map(|path| path.page));
        match pages.next() {
            Some(first) => pages.all(|page| page == first),
            None => true,
        }
    }
}

/// Moves every member by `(dx, dy)` points, as one undo step. Each member
/// keeps its size: only the origin of its destination changes.
pub fn move_selection(selection: &Selection, dx: f64, dy: f64) -> Option<Command> {
    let shift = |bbox: Rect| Rect {
        x: bbox.x + dx,
        y: bbox.y + dy,
        ..bbox
    };
    let runs = selection.runs.iter().map(|run| Command::MoveTextRun {
        item: run.clone(),
        to: shift(run.bbox),
    });
    let images = selection.images.iter().map(|image| Command::MoveImage {
        item: image.clone(),
        to: shift(image.bbox),
    });
    let paths = selection.paths.iter().map(|path| Command::MovePath {
        item: path.clone(),
        to: shift(path.bbox),
    });
    batch(runs.chain(images).chain(paths).collect())
}

/// Removes every member, as one undo step.
pub fn remove_selection(selection: &Selection) -> Option<Command> {
    let runs = selection.runs.iter().cloned().map(Command::RemoveTextRun);
    let images = selection.images.iter().map(|image| Command::RemoveImage {
        item: image.clone(),
        source: None,
    });
    let paths = selection.paths.iter().cloned().map(Command::RemovePath);
    batch(runs.chain(images).chain(paths).collect())
}

fn batch(children: Vec<Command>) -> Option<Command> {
    (!children.is_empty()).then_some(Command::Batch(children))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{ContentItemId, FontKind, ImageSource, PageId, PathPaint};

    fn rect(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 40.0,
            height: 12.0,
        }
    }

    fn run(id: u64, page: u32) -> TextRun {
        TextRun {
            id: ContentItemId(id),
            page: PageId(page),
            bbox: rect(10.0, 20.0),
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::Standard14,
            text: "cell".to_string(),
        }
    }

    fn image(id: u64) -> ImageItem {
        ImageItem {
            id: ContentItemId(id),
            page: PageId(0),
            bbox: rect(100.0, 200.0),
            source: ImageSource::Resource("Im1".to_string()),
        }
    }

    fn path(id: u64) -> PathItem {
        PathItem {
            id: ContentItemId(id),
            page: PageId(0),
            bbox: rect(5.0, 15.0),
            paint: PathPaint::Stroke,
        }
    }

    #[test]
    fn a_move_shifts_every_kind_by_the_same_offset_and_keeps_sizes() {
        let selection = Selection {
            runs: vec![run(1, 0)],
            images: vec![image(2)],
            paths: vec![path(3)],
        };

        let Some(Command::Batch(children)) = move_selection(&selection, 7.0, -3.0) else {
            panic!("a selection move is one batch");
        };

        assert_eq!(
            children,
            [
                Command::MoveTextRun {
                    item: run(1, 0),
                    to: rect(17.0, 17.0),
                },
                Command::MoveImage {
                    item: image(2),
                    to: rect(107.0, 197.0),
                },
                Command::MovePath {
                    item: path(3),
                    to: rect(12.0, 12.0),
                },
            ]
        );
    }

    #[test]
    fn a_removal_names_every_member_once() {
        let selection = Selection {
            runs: vec![run(1, 0)],
            images: vec![image(2)],
            paths: vec![path(3)],
        };

        let Some(Command::Batch(children)) = remove_selection(&selection) else {
            panic!("a selection removal is one batch");
        };

        assert_eq!(
            children,
            [
                Command::RemoveTextRun(run(1, 0)),
                Command::RemoveImage {
                    item: image(2),
                    source: None,
                },
                Command::RemovePath(path(3)),
            ]
        );
        assert!(children.iter().all(Command::is_batchable));
    }

    #[test]
    fn an_empty_selection_edits_nothing() {
        assert_eq!(move_selection(&Selection::default(), 1.0, 1.0), None);
        assert_eq!(remove_selection(&Selection::default()), None);
    }

    #[test]
    fn members_on_two_pages_are_not_one_page() {
        let mixed = Selection {
            runs: vec![run(1, 0), run(2, 1)],
            ..Selection::default()
        };
        assert!(!mixed.is_on_one_page());
        assert!(Selection {
            runs: vec![run(1, 0)],
            images: vec![image(2)],
            paths: vec![path(3)],
        }
        .is_on_one_page());
    }
}
