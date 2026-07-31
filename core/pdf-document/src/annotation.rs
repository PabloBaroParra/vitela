//! Annotation data model (T-011): highlight/underline/strikeout/ink/
//! text-note/shape/stamp as pure data, plus the `AnnotationSet` collection
//! that `Document` owns.
//!
//! Building the actual PDF annotation objects (content/appearance streams)
//! is `pdf-annotate`'s job (Batch 5) — this crate only models the data.

use crate::document::PageId;

/// Identifies an annotation within a `Document`, independent of `PageId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnnotationId(pub u64);

/// Axis-aligned rectangle in PDF page-space (points, origin bottom-left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// RGB color, 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// The `/Popup` half of a text-note annotation pair (spec "Text Note Popup
/// Linking"): carries the open/closed state and comment body. The `/Parent`
/// back-reference to the owning markup annotation is implicit — the popup
/// always lives inside `AnnotationKind::TextNote` alongside its anchor, it
/// is never a bare top-level annotation and never uses `/IRT`.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    pub is_open: bool,
    pub contents: String,
}

/// The kind-specific data for an annotation.
///
/// `#[non_exhaustive]`: a signatures scope change is on the roadmap
/// (drawn/stamped signatures reuse `Ink`/`Stamp`, but a future cryptographic
/// signature-appearance variant may need its own case) — marking this
/// non-exhaustive costs nothing today and avoids a breaking change later.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AnnotationKind {
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
    Ink {
        points: Vec<(f64, f64)>,
        color: Color,
    },
    TextNote {
        rect: Rect,
        contents: String,
        popup: Popup,
    },
    Shape {
        rect: Rect,
        color: Color,
    },
    /// Image-based stamp appearance (spec "Image Stamp Annotations" /
    /// "Insert Image from Bytes"). `image_bytes` holds the already-decoded
    /// source image; building the `/AP` + `/SMask` appearance stream is
    /// `pdf-annotate`'s job.
    Stamp {
        rect: Rect,
        image_bytes: Vec<u8>,
        has_alpha: bool,
    },
}

/// A single annotation attached to a page.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub id: AnnotationId,
    pub page: PageId,
    pub kind: AnnotationKind,
}

/// The collection of annotations owned by a `Document`.
///
/// Backed by a `Vec` (MVP annotation counts per document are small; O(n)
/// lookup by id is not a bottleneck) rather than a `HashMap` so ordering is
/// preserved for deterministic save output (spec "Cross-Platform Feature
/// Parity" byte-identical CI check).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnnotationSet {
    annotations: Vec<Annotation>,
}

impl AnnotationSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an annotation. Does not check for duplicate ids — callers
    /// (EditLog command application) are responsible for id uniqueness.
    pub fn insert(&mut self, annotation: Annotation) {
        self.annotations.push(annotation);
    }

    /// Removes and returns the annotation with the given id, if present.
    pub fn remove(&mut self, id: AnnotationId) -> Option<Annotation> {
        let index = self.annotations.iter().position(|a| a.id == id)?;
        Some(self.annotations.remove(index))
    }

    /// Swaps in a new value for the annotation that already carries
    /// `annotation`'s id, **keeping its position**, and returns the previous
    /// value. Returns `None` and leaves the set untouched when no annotation
    /// has that id.
    ///
    /// A remove-then-insert pair would move the annotation to the end of the
    /// set, which this type's ordering guarantee (see the type doc) does not
    /// allow: it changes paint order and makes apply-then-undo stop being an
    /// identity on the set. Edits that keep an annotation's identity — move,
    /// resize, restyle — go through here.
    pub fn replace(&mut self, annotation: Annotation) -> Option<Annotation> {
        let slot = self
            .annotations
            .iter_mut()
            .find(|existing| existing.id == annotation.id)?;
        Some(std::mem::replace(slot, annotation))
    }

    pub fn get(&self, id: AnnotationId) -> Option<&Annotation> {
        self.annotations.iter().find(|a| a.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Annotation> {
        self.annotations.iter()
    }

    pub fn len(&self) -> usize {
        self.annotations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.annotations.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_annotation(id: u64) -> Annotation {
        Annotation {
            id: AnnotationId(id),
            page: PageId(0),
            kind: AnnotationKind::Highlight {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
                color: Color {
                    r: 255,
                    g: 255,
                    b: 0,
                },
            },
        }
    }

    #[test]
    fn insert_then_get_returns_the_annotation() {
        let mut set = AnnotationSet::new();
        set.insert(sample_annotation(1));

        assert_eq!(set.len(), 1);
        assert!(set.get(AnnotationId(1)).is_some());
    }

    #[test]
    fn remove_returns_and_deletes_the_annotation() {
        let mut set = AnnotationSet::new();
        set.insert(sample_annotation(1));

        let removed = set.remove(AnnotationId(1)).expect("should be present");
        assert_eq!(removed.id, AnnotationId(1));
        assert!(set.is_empty());
        assert!(set.get(AnnotationId(1)).is_none());
    }

    #[test]
    fn remove_missing_id_returns_none() {
        let mut set = AnnotationSet::new();
        assert!(set.remove(AnnotationId(42)).is_none());
    }

    #[test]
    fn replace_keeps_the_annotation_in_place() {
        let mut set = AnnotationSet::new();
        set.insert(sample_annotation(1));
        set.insert(sample_annotation(2));
        set.insert(sample_annotation(3));
        let mut edited = sample_annotation(2);
        edited.page = PageId(7);

        let previous = set.replace(edited).expect("id 2 is present");

        assert_eq!(previous.page, PageId(0));
        assert_eq!(
            set.iter().map(|a| a.id).collect::<Vec<_>>(),
            vec![AnnotationId(1), AnnotationId(2), AnnotationId(3)],
            "an edit must not move the annotation to the end of the set"
        );
        assert_eq!(set.get(AnnotationId(2)).expect("present").page, PageId(7));
    }

    #[test]
    fn replace_missing_id_returns_none_and_adds_nothing() {
        let mut set = AnnotationSet::new();
        set.insert(sample_annotation(1));

        assert!(set.replace(sample_annotation(42)).is_none());
        assert_eq!(set.len(), 1);
    }
}
