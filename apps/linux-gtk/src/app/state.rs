//! Shared data types for the shell: the widget handle bundle, per-document
//! session state, and the small value types the feature modules pass around.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gio, Box as GtkBox, Button, Dialog, DrawingArea, Entry, Label, Overlay, Picture,
    ScrolledWindow, ToggleButton,
};
use pdf_document::{AnnotationId, Document};
use pdf_manip::LopdfDocument;
use pdf_render::{CancellationHandle, DocumentHandle, PageCharacters, TextMatch};

/// Where an open request's bytes come from.
///
/// The password-retry loop re-opens the *same* source, so this has to be
/// cheap to clone and outlive the first attempt — hence an owned path rather
/// than a borrow, and a `'static` slice for the compiled-in sample.
#[derive(Clone)]
pub(crate) enum DocumentSource {
    /// A file the user picked. GTK4 is the one shell that can hand pdfium a
    /// real filesystem path (see `PdfiumRenderer::open_document`).
    File(PathBuf),
    /// The sample document baked into the binary at compile time.
    Embedded(&'static [u8]),
}

#[derive(Clone)]
pub(crate) struct Viewer {
    pub(crate) scroll: ScrolledWindow,
    pub(crate) pages: GtkBox,
    /// Brand mark overlaid on the page area. Visible exactly while there is
    /// nothing to show — see `brand::build_app_mark`.
    pub(crate) app_mark: Picture,
    pub(crate) status: Label,
    pub(crate) search_entry: Entry,
    pub(crate) find_previous: Button,
    pub(crate) find_next: Button,
    pub(crate) print_button: Button,
    pub(crate) save_button: Button,
    pub(crate) undo_action: gio::SimpleAction,
    pub(crate) redo_action: gio::SimpleAction,
    pub(crate) annotation_buttons: AnnotationToolbar,
    pub(crate) state: Rc<RefCell<ViewerState>>,
}

impl Viewer {
    /// The message to show instead of extracting the open document's text, or
    /// `None` when extraction is allowed.
    ///
    /// The single place the shell asks the permission question. Search asks it
    /// before querying pdfium; text selection asks it before putting a run on
    /// the clipboard. With no document open there is nothing to refuse — the
    /// caller reports that case in its own words.
    pub(crate) fn text_extraction_refusal(&self) -> Option<&'static str> {
        self.state
            .borrow()
            .session
            .as_ref()
            .and_then(|session| session.text_access.refusal())
    }

    pub(crate) fn annotation_editing_refusal(&self) -> Option<&'static str> {
        self.state
            .borrow()
            .session
            .as_ref()
            .and_then(|session| session.annotation_access.refusal())
    }
}

pub(crate) struct ViewerState {
    pub(crate) generation: u64,
    pub(crate) session: Option<DocumentSession>,
    /// The creation tool armed on the toolbar, if any. A drag on a page while
    /// this is set draws an annotation instead of selecting text.
    ///
    /// Shell mode rather than document state: it deliberately outlives the
    /// open document, so opening a second PDF does not silently disarm the
    /// tool the user just picked.
    pub(crate) active_tool: Option<Tool>,
    /// The password prompt for the in-flight open attempt, if any. Tracked so
    /// a new open request (which may supersede this one before the user has
    /// answered) can tear down the stale prompt instead of leaving it
    /// stacked underneath a second one — see `document::begin_loading` and
    /// `document::dismiss_password_dialog`.
    pub(crate) password_dialog: Option<Dialog>,
}

/// Inputs that must remain paired with the editable model for a valid save.
#[derive(Clone)]
pub(crate) struct SaveBacking {
    pub(crate) base: LopdfDocument,
    pub(crate) original_bytes: Vec<u8>,
    pub(crate) password: Option<String>,
}

/// Identifies the exact model revision from which asynchronous work started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SessionToken {
    pub(crate) generation: u64,
    pub(crate) edit_revision: u64,
}

impl SessionToken {
    pub(crate) fn matches(self, generation: u64, edit_revision: u64) -> bool {
        self.generation == generation && self.edit_revision == edit_revision
    }
}

/// Whether this document's text may be extracted — read once at open time
/// and cached for the session, because it cannot change while the document
/// stays open.
///
/// The shell links `pdf-render` directly and so never crosses the `pdf-ffi`
/// boundary where the other shells' gate lives; this is that gate. Anything
/// that would hand the user the document's text — search, and the selection
/// clipboard — must ask [`TextAccess::refusal`] first.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum TextAccess {
    /// Unencrypted, or the permissions (or an owner credential) allow it.
    Allowed,
    /// The document's `/P` withholds the copy-or-extract permission.
    Forbidden,
    /// The document's security could not be read at all. Refused rather than
    /// assumed permissive: an unreadable policy is not a permissive one. The
    /// document still renders — failing to classify it must not cost the user
    /// the ability to *view* it.
    Unreadable,
}

impl TextAccess {
    /// The message to show instead of extracting text, or `None` when
    /// extraction is allowed.
    pub(crate) fn refusal(self) -> Option<&'static str> {
        match self {
            TextAccess::Allowed => None,
            TextAccess::Forbidden => {
                Some("This document does not permit copying or extracting its text.")
            }
            TextAccess::Unreadable => Some(
                "This document's permissions could not be read, so its text cannot be extracted.",
            ),
        }
    }
}

/// One annotation type the toolbar can create.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Tool {
    Highlight,
    Underline,
    Strikeout,
    Ink,
    TextNote,
    Shape,
    Stamp,
}

impl Tool {
    /// Every creation tool — the single source of truth for the buttons that
    /// get built and the handlers wired to them, so the two cannot drift.
    pub(crate) const ALL: [Tool; 7] = [
        Tool::Highlight,
        Tool::Underline,
        Tool::Strikeout,
        Tool::Ink,
        Tool::TextNote,
        Tool::Shape,
        Tool::Stamp,
    ];

    /// The tool's button label, also used to name it in status messages. A
    /// `match` rather than a lookup table so adding a variant fails to
    /// compile until it is named.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Tool::Highlight => "Highlight",
            Tool::Underline => "Underline",
            Tool::Strikeout => "Strikeout",
            Tool::Ink => "Ink",
            Tool::TextNote => "Note",
            Tool::Shape => "Shape",
            Tool::Stamp => "Stamp",
        }
    }

    /// Whether this tool marks up existing text rather than drawing a shape.
    ///
    /// These three are the PDF text-markup kinds, and they are the ones that
    /// can be applied straight to a text selection: the user has already said
    /// which words they mean, so asking them to re-trace the same words with a
    /// drag would be busywork.
    pub(crate) fn marks_up_text(self) -> bool {
        matches!(self, Tool::Highlight | Tool::Underline | Tool::Strikeout)
    }

    /// Whether a freehand drag of this tool produces a straight rule rather
    /// than a region.
    ///
    /// An underline and a strikeout *are* lines, so how far the pointer
    /// drifted vertically says nothing about them — only how far it travelled
    /// along the line does. A highlight is a band and does use both axes.
    ///
    /// This is about the freehand drag only. Applied to a text selection these
    /// same tools take the selected line's own rect, and the rule is placed
    /// relative to it.
    pub(crate) fn draws_a_rule(self) -> bool {
        matches!(self, Tool::Underline | Tool::Strikeout)
    }

    /// Whether this tool is drawn as a freehand path rather than a rectangle.
    /// Ink is the one kind `pdf-annotate` models as a polyline, so it is the
    /// one kind whose placement drag records every point the pointer visited.
    pub(crate) fn is_freehand(self) -> bool {
        matches!(self, Tool::Ink)
    }
}

/// The annotation toolbar's buttons, held by name rather than by position.
///
/// An earlier shape was a flat `Vec<Button>` indexed with literals at three
/// separate call sites; inserting a button in the middle would have silently
/// rewired every handler after it, with nothing for the compiler to catch.
#[derive(Clone)]
pub(crate) struct AnnotationToolbar {
    /// One creation button per tool, paired with the tool it arms. These are
    /// toggles, not push buttons: creating an annotation takes two steps —
    /// arm the tool here, then draw it on the page.
    pub(crate) create: Vec<(Tool, ToggleButton)>,
    pub(crate) select_previous: Button,
    pub(crate) move_selection: Button,
    pub(crate) resize_selection: Button,
    pub(crate) restyle_selection: Button,
    pub(crate) delete_selection: Button,
    /// The Delete-key half of the delete button. Kept here so the same place
    /// that decides whether the button is usable decides whether the
    /// accelerator is live — see `annotations::update_annotation_controls`.
    pub(crate) delete_action: gio::SimpleAction,
}

/// Which corner of a selected annotation a resize drag has hold of.
///
/// Named by PDF-space position, where `y` grows upward — so `BottomLeft` is
/// the corner at the rect's own `(x, y)`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Corner {
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
}

impl Corner {
    pub(crate) const ALL: [Corner; 4] = [
        Corner::BottomLeft,
        Corner::BottomRight,
        Corner::TopLeft,
        Corner::TopRight,
    ];
}

/// What a drag on an already-selected annotation is doing to it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum AnnotationDragMode {
    /// Sliding the whole annotation, body grabbed anywhere inside it.
    Move,
    /// Pulling one corner, with the opposite one held still.
    Resize(Corner),
}

/// A selected annotation being moved or resized right now.
///
/// Like [`Placement`], it lives on the session and reaches the `EditLog` only
/// when the pointer comes up: a drag in progress is not an edit yet, and an
/// abandoned one must leave no trace.
pub(crate) struct AnnotationDrag {
    pub(crate) id: AnnotationId,
    pub(crate) mode: AnnotationDragMode,
    /// Where the pointer went down, in PDF page space.
    pub(crate) origin: (f64, f64),
    /// Where the pointer is now, in PDF page space.
    pub(crate) current: (f64, f64),
}

/// An annotation being drawn on a page right now, between the pointer going
/// down and coming back up.
///
/// Held on the session rather than committed immediately because it is not an
/// edit yet: nothing reaches the `EditLog` until the drag ends. Replacing the
/// document drops any half-drawn annotation with it.
pub(crate) struct Placement {
    pub(crate) tool: Tool,
    pub(crate) page_index: usize,
    /// Where the pointer went down, in PDF page space.
    pub(crate) origin: (f64, f64),
    /// Where the pointer is now, in PDF page space.
    pub(crate) current: (f64, f64),
    /// Every position the pointer has visited, in PDF page space. Recorded
    /// only for freehand tools (see [`Tool::is_freehand`]); a rectangular tool
    /// needs nothing but `origin` and `current`.
    pub(crate) points: Vec<(f64, f64)>,
}

/// Shown when the document allows annotation changes but this shell could not
/// build an editable model for it. Lives here as a constant because both
/// [`AnnotationAccess::refusal`] and the model unwrap in `app::annotations`
/// report it, and a document that says two different things about why its
/// toolbar is dead is worse than one that says nothing.
pub(crate) const ANNOTATION_MODEL_UNAVAILABLE: &str =
    "This document could not be prepared for annotation changes.";

/// Whether this document's annotations may be edited — the annotation twin of
/// [`TextAccess`], read once at open time and cached for the session.
///
/// Kept separate from `document_model.is_some()` on purpose: "the document
/// forbids this" and "this shell could not load it" are different facts, and
/// collapsing them makes the shell claim a permission restriction the document
/// never declared.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum AnnotationAccess {
    /// Unencrypted, or the permissions (or an owner credential) allow it, and
    /// the editable model was built.
    Allowed,
    /// The document's `/P` withholds the annotate permission.
    Forbidden,
    /// The permission question could not be answered, or the editable model
    /// could not be built. Refused rather than assumed permissive, for the
    /// same reason as [`TextAccess::Unreadable`]; the document still renders.
    Unavailable,
}

impl AnnotationAccess {
    /// The message to show instead of editing annotations, or `None` when
    /// editing is allowed.
    pub(crate) fn refusal(self) -> Option<&'static str> {
        match self {
            AnnotationAccess::Allowed => None,
            AnnotationAccess::Forbidden => {
                Some("This document does not permit annotation changes.")
            }
            AnnotationAccess::Unavailable => Some(ANNOTATION_MODEL_UNAVAILABLE),
        }
    }
}

pub(crate) struct DocumentSession {
    pub(crate) document: DocumentHandle,
    /// Whether search and text selection may read this document's text.
    pub(crate) text_access: TextAccess,
    pub(crate) annotation_access: AnnotationAccess,
    /// The editable core model. Rendering remains backed by pdfium until a
    /// future save/reopen refresh, but every annotation command is recorded in
    /// this model's EditLog immediately.
    pub(crate) document_model: Option<Document>,
    pub(crate) save_backing: Option<SaveBacking>,
    pub(crate) edit_revision: u64,
    pub(crate) next_annotation_id: u64,
    pub(crate) selected_annotation: Option<AnnotationId>,
    /// The annotation being drawn right now, if a placement drag is in flight.
    pub(crate) placement: Option<Placement>,
    /// The selected annotation being moved or resized right now, if any.
    pub(crate) annotation_drag: Option<AnnotationDrag>,
    pub(crate) physical_width: u32,
    pub(crate) physical_height: u32,
    pub(crate) scale_factor: i32,
    pub(crate) pages: Vec<PageSlot>,
    /// Cached logical heights, one per page — recomputed only when the fit
    /// changes (`show_document`/`refresh_layout`) so the per-scroll
    /// `update_viewport` never re-queries every widget's size request.
    pub(crate) page_heights: Vec<i32>,
    /// The last `(first, last)` range reported to the status label, so a
    /// scroll tick that doesn't move the visible range skips the redundant
    /// `format!` + `set_text`.
    pub(crate) last_visible: Option<(usize, usize)>,
    /// Matches for the last query run against *this* document. Lives in the
    /// session so replacing the document drops them with it.
    pub(crate) search: Option<SearchState>,
    /// Id of the most recently issued search. A slow search whose id is no
    /// longer current has been superseded by a later query and must not
    /// overwrite its results.
    pub(crate) next_search_id: u64,
    /// The current drag-selection, if any. Lives in the session so replacing
    /// the document drops it with the text it addressed.
    pub(crate) selection: Option<Selection>,
    pub(crate) active: HashMap<usize, ActiveRender>,
    pub(crate) next_render_id: u64,
    pub(crate) zoom: super::layout::Zoom,
    pub(crate) zoom_generation: u64,
    pub(crate) active_tiles: HashMap<usize, ActiveRender>,
}

pub(crate) struct SearchState {
    pub(crate) query: String,
    pub(crate) matches: Vec<TextMatch>,
    pub(crate) current: usize,
}

/// A drag-selection on one page, stored as the two PDF-space points the
/// pointer touched rather than as resolved carets.
///
/// Points, because a page's characters load asynchronously: a drag that
/// starts before its text arrives still records where it began, and the
/// carets resolve on the first paint after the load lands. Resolved carets
/// would have to be back-filled by the load handler instead, which then has
/// to know whether the drag it is completing is still the current one.
///
/// One page, not a document-wide range: a selection is only ever consumed
/// per page — by the paint below and by the text-markup annotations of T-047,
/// whose `/Rect` lives on a single page by construction.
pub(crate) struct Selection {
    pub(crate) page_index: usize,
    pub(crate) anchor: (f32, f32),
    pub(crate) focus: (f32, f32),
}

pub(crate) struct ActiveRender {
    pub(crate) id: u64,
    pub(crate) generation: u64,
    pub(crate) cancellation: CancellationHandle,
}

pub(crate) struct PageSlot {
    pub(crate) overlay: Overlay,
    pub(crate) picture: Picture,
    /// Transparent layer painting this page's selection and search matches.
    /// It is also the drag target — being the topmost overlay child, it is
    /// what the pointer lands on.
    pub(crate) highlights: DrawingArea,
    /// This page's characters, loaded on first use. `None` means "not loaded
    /// yet", which `characters_requested` disambiguates from "load in
    /// flight" so a drag over a page cannot queue one job per motion event.
    pub(crate) characters: Option<PageCharacters>,
    pub(crate) characters_requested: bool,
    pub(crate) width_pt: f32,
    pub(crate) height_pt: f32,
    pub(crate) state: PageState,
    pub(crate) target_dpi: u32,
    pub(crate) budget: super::layout::TileBudget,
    pub(crate) tiles: HashMap<super::layout::TileRect, Picture>,
    pub(crate) tile_dpi: u32,
    pub(crate) tile_generation: u64,
    /// DPI whose tile batch failed, or 0. Terminal for that DPI so a doomed
    /// batch isn't re-queued on every scroll tick; a new zoom clears it.
    pub(crate) tile_failed_dpi: u32,
}

/// Render lifecycle of a single page slot. `Skipped`/`Failed` are terminal
/// for the current fit: they keep `update_viewport` from re-queuing a job
/// that can only be rejected or fail again. A new fit (`refresh_layout`)
/// resets every slot to `Idle`, giving oversized/failed pages one retry at
/// the new size.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum PageState {
    Idle,
    Rendered,
    Skipped,
    Failed,
}

/// A rendered page in `Send` form: produced on a worker thread, converted
/// into a non-`Send` pixbuf only on the GTK main thread.
pub(crate) struct RenderedPage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: u32,
    pub(crate) pixels: Vec<u8>,
}

pub(crate) struct OpenedDocument {
    pub(crate) document: DocumentHandle,
    pub(crate) page_sizes: Vec<(f32, f32)>,
    pub(crate) text_access: TextAccess,
    pub(crate) annotation_access: AnnotationAccess,
    pub(crate) document_model: Option<Document>,
    pub(crate) save_backing: Option<SaveBacking>,
}

#[cfg(test)]
mod tests {
    use super::{AnnotationAccess, SessionToken, TextAccess, ANNOTATION_MODEL_UNAVAILABLE};

    #[test]
    fn a_session_token_rejects_a_newer_generation_or_edit_revision() {
        let captured = SessionToken {
            generation: 4,
            edit_revision: 2,
        };

        assert!(captured.matches(4, 2));
        assert!(!captured.matches(5, 2));
        assert!(!captured.matches(4, 3));
    }

    #[test]
    fn only_allowed_access_skips_the_refusal() {
        assert!(TextAccess::Allowed.refusal().is_none());
    }

    #[test]
    fn an_unreadable_policy_refuses_just_like_a_forbidding_one() {
        // Fail closed: a document whose security could not be read is not a
        // document that granted permission.
        assert!(TextAccess::Forbidden.refusal().is_some());
        assert!(TextAccess::Unreadable.refusal().is_some());
    }

    #[test]
    fn each_refusal_explains_its_own_cause() {
        assert_ne!(
            TextAccess::Forbidden.refusal(),
            TextAccess::Unreadable.refusal()
        );
    }

    #[test]
    fn only_allowed_annotation_access_skips_the_refusal() {
        assert!(AnnotationAccess::Allowed.refusal().is_none());
    }

    #[test]
    fn an_unavailable_model_refuses_just_like_a_forbidding_document() {
        // Fail closed, for the same reason `TextAccess::Unreadable` does.
        assert!(AnnotationAccess::Forbidden.refusal().is_some());
        assert!(AnnotationAccess::Unavailable.refusal().is_some());
    }

    /// A document that withholds the annotate permission and one this shell
    /// merely failed to model are different facts, and the user is told which
    /// one they hit. Collapsing them would have the shell report a restriction
    /// the document never declared.
    #[test]
    fn a_permission_refusal_never_reads_like_a_load_failure() {
        assert_ne!(
            AnnotationAccess::Forbidden.refusal(),
            AnnotationAccess::Unavailable.refusal()
        );
        assert_eq!(
            AnnotationAccess::Unavailable.refusal(),
            Some(ANNOTATION_MODEL_UNAVAILABLE)
        );
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FitRequest {
    pub(crate) available_width: u32,
    pub(crate) available_height: u32,
    pub(crate) scale_factor: i32,
}

impl FitRequest {
    pub(crate) fn measure(viewer: &Viewer) -> Self {
        let scale_factor = viewer.scroll.scale_factor().max(1);
        FitRequest {
            available_width: (viewer.scroll.width().max(1) * scale_factor) as u32,
            available_height: (viewer.scroll.height().max(1) * scale_factor) as u32,
            scale_factor,
        }
    }

    /// The same measurement expressed the way the layout math wants it:
    /// logical pixels, with the display scale alongside.
    pub(crate) fn viewport(self) -> Viewport {
        let scale = self.scale_factor.max(1) as u32;
        Viewport {
            logical_width: f64::from(self.available_width / scale),
            logical_height: f64::from(self.available_height / scale),
            scale_factor: f64::from(self.scale_factor),
        }
    }
}

/// The area pages are fitted into, in logical pixels, plus the display scale
/// their bitmaps are rasterised at.
///
/// These three travel together everywhere, so they travel as one value. Passed
/// separately they were three bare `f64`s in a row at every call site, where
/// nothing but argument order stops width and height being swapped — a
/// mix-up the compiler cannot catch and that silently mis-fits every page. The
/// Windows shell carries the same measurement in `ViewportSize` for the same
/// reason.
#[derive(Clone, Copy)]
pub(crate) struct Viewport {
    pub(crate) logical_width: f64,
    pub(crate) logical_height: f64,
    pub(crate) scale_factor: f64,
}
