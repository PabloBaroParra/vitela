//! Shared data types for the shell: the widget handle bundle, per-document
//! session state, and the small value types the feature modules pass around.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    cairo, gio, Box as GtkBox, Button, DrawingArea, DropDown, Entry, Label, Overlay, Picture,
    ScrolledWindow, SpinButton, ToggleButton, Window,
};
use pdf_document::{
    AnnotationId, Document, FormFieldId, ImageItem, PageContent, PdfDateOffset, TextRun,
};
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
    /// A PDF created in memory, such as Ctrl+N's new blank document.
    Bytes(Vec<u8>),
}

#[derive(Clone)]
pub(crate) struct Viewer {
    pub(crate) scroll: ScrolledWindow,
    pub(crate) pages: GtkBox,
    /// The left-side page navigator. Its contents mirror the current document
    /// session, while the session itself remains owned by [`ViewerState`].
    pub(crate) page_navigation: GtkBox,
    /// The right panel's editable document-properties fields (T-176), kept
    /// updated by `metadata::refresh` — see `MetadataPanel`.
    pub(crate) metadata: MetadataPanel,
    /// Brand mark overlaid on the page area. Visible exactly while there is
    /// nothing to show — see `brand::build_app_mark`.
    pub(crate) app_mark: Picture,
    pub(crate) status: Label,
    /// "3 / 12" readout of the current `last_visible` range, kept in step by
    /// `render::update_viewport` alongside `status`.
    pub(crate) page_indicator: Label,
    /// "100%" readout of `layout::current_zoom_factor`, kept in step by
    /// `layout::refresh_layout`.
    pub(crate) zoom_label: Label,
    pub(crate) search_entry: Entry,
    pub(crate) find_previous: Button,
    pub(crate) find_next: Button,
    pub(crate) print_button: Button,
    pub(crate) save_button: Button,
    pub(crate) undo_action: gio::SimpleAction,
    pub(crate) redo_action: gio::SimpleAction,
    pub(crate) annotation_buttons: AnnotationToolbar,
    pub(crate) content_edit_button: ToggleButton,
    /// Arms "click anywhere to open a blank text editor" sub-mode (T-163),
    /// mutually exclusive with `insert_image_button`. Sensitivity is owned
    /// by `content_edit::update_controls`, the same gate as
    /// `content_edit_button` itself — inserting is only meaningful once
    /// content-edit mode itself is available.
    pub(crate) insert_text_button: ToggleButton,
    /// Arms "click anywhere to insert a picked image" sub-mode (T-163), the
    /// twin of `insert_text_button` for images. Same sensitivity gate.
    pub(crate) insert_image_button: ToggleButton,
    /// Deletes the selected image in content-edit mode (T-162 Slice 1).
    /// Sensitivity is owned by `update_content_edit_controls`, the
    /// content-edit twin of `annotations::toolbar::update_annotation_controls`.
    pub(crate) delete_image_button: Button,
    /// Opens a file picker and swaps the selected image's bytes (T-162
    /// Slice 2). Sensitivity is owned by `update_content_edit_controls`
    /// alongside `delete_image_button` — the same selection gates both.
    pub(crate) replace_image_button: Button,
    /// The forms toolbar (T-141): the mode toggle, the four placement
    /// toggles, and the style inspector for the selected field.
    pub(crate) forms: FormFieldToolbar,
    /// Opens the `.pfx`/`.p12` chooser and password prompt (Batch B23 Fase
    /// 2). Lives on the "Fill & Sign" tab alongside `forms`, but is its own
    /// field rather than folded into `FormFieldToolbar`: signing identities
    /// are not form fields, and `sign` is expected to grow its own toolbar
    /// struct once Fase 3 (PKCS#11) and Fase 4 (the identity picker) land.
    pub(crate) choose_signing_certificate: Button,
    /// Opens the PKCS#11 module discovery/PIN flow (Batch B23 Fase 3) —
    /// `choose_signing_certificate`'s twin for a card or token instead of a
    /// `.pfx` file. Lives alongside it for the same reason.
    pub(crate) choose_pkcs11_certificate: Button,
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

    pub(crate) fn content_edit_refusal(&self) -> Option<&'static str> {
        self.state
            .borrow()
            .session
            .as_ref()
            .and_then(|session| session.content_edit_access.refusal())
    }
}

pub(crate) struct ViewerState {
    pub(crate) generation: u64,
    /// Changes only when `show_document` replaces the visible document.
    pub(crate) session_id: u64,
    pub(crate) session: Option<DocumentSession>,
    /// The creation tool armed on the toolbar, if any. A drag on a page while
    /// this is set draws an annotation instead of selecting text.
    ///
    /// Shell mode rather than document state: it deliberately outlives the
    /// open document, so opening a second PDF does not silently disarm the
    /// tool the user just picked.
    pub(crate) active_tool: Option<Tool>,
    /// Whether a page click opens the inline content editor instead of
    /// selecting text or placing an annotation. Lives here rather than on
    /// `DocumentSession` for the same reason `active_tool` does: it is a
    /// shell mode, not document state, so it deliberately outlives the open
    /// document. Mutually exclusive with `active_tool` — arming either one
    /// clears the other (`content_edit::set_mode`, `annotations::toolbar::arm_tool`).
    pub(crate) content_edit_mode: bool,
    /// Which kind of new content a content-edit-mode click inserts, if any
    /// (T-163) — `None` means an ordinary click still targets an existing
    /// run/image, same as before this sub-mode existed.
    ///
    /// Lives here rather than on `DocumentSession` for the same reason
    /// `content_edit_mode` does: it is a shell mode, not document state, so
    /// it deliberately outlives the open document. In practice it is always
    /// cleared alongside `content_edit_mode` (`content_edit::set_mode`), so
    /// the two never disagree about whether content-edit mode is active —
    /// only about what a click inside it does.
    pub(crate) content_insert_mode: Option<ContentInsertKind>,
    /// Whether a `document::refresh_after_content_edit` preview refresh
    /// (save-to-buffer, reopen, rebuild every page widget) is currently
    /// running.
    ///
    /// Content-edit commits are recorded from two independent sites that
    /// never coordinate with each other — `content_edit::editor::commit`
    /// (retyping a run) and `content_edit::text::finish_text_drag` (dragging
    /// one) — so two refreshes can be requested back to back before the
    /// first one's `show_document` has finished tearing down and rebuilding
    /// `viewer.pages`. A second refresh starting mid-rebuild races the first
    /// one on that same `GtkBox`, which is unsafe: both `while let Some(child)
    /// = viewer.pages.first_child()` teardown and its rebuild `append` can
    /// observe a widget the other side is mutating. `refresh_after_content_edit`
    /// checks this flag and defers instead of starting a concurrent rebuild.
    pub(crate) content_refresh_in_flight: bool,
    /// A refresh message queued because one arrived while
    /// `content_refresh_in_flight` was already set. Replayed once the
    /// in-flight refresh finishes, so the second edit's preview still lands
    /// instead of being silently dropped.
    pub(crate) content_refresh_pending: Option<&'static str>,
    /// Whether a page click targets a form field instead of selecting text or
    /// placing an annotation (T-141). Shell mode, not document state, for the
    /// same reason `content_edit_mode` is — see `forms::set_mode`. Mutually
    /// exclusive with `active_tool` and `content_edit_mode`.
    pub(crate) form_edit_mode: bool,
    /// Which kind of new field a forms-edit-mode click places, if any
    /// (T-141) — the forms twin of `content_insert_mode`. `None` means an
    /// ordinary click targets an existing field (select, move, resize)
    /// instead of creating one.
    pub(crate) form_field_kind: Option<FieldKind>,
    /// The password prompt for the in-flight open attempt, if any. Tracked so
    /// a new open request (which may supersede this one before the user has
    /// answered) can tear down the stale prompt instead of leaving it
    /// stacked underneath a second one — see `document::begin_loading` and
    /// `document::dismiss_password_dialog`.
    pub(crate) password_dialog: Option<Window>,
    /// The `.pfx`/`.p12` password prompt for the in-flight certificate load,
    /// if any (Batch B23 Fase 2) — the signing twin of `password_dialog`,
    /// same reason: a later attempt can supersede this one before the
    /// background load resolves, and the stale attempt's result must not
    /// clobber the newer one's — see `sign::dismiss_pfx_dialog`.
    pub(crate) pfx_dialog: Option<Window>,
    /// The PKCS#11 PIN prompt for the in-flight token load, if any (Batch B23
    /// Fase 3) — `pfx_dialog`'s twin for the card/token flow, same reason.
    pub(crate) pkcs11_dialog: Option<Window>,
    /// The identity picker opened once a `.pfx` password or a PKCS#11 PIN
    /// unlocks at least one identity (Batch B23 Fase 4) — tracked for the
    /// same supersede/dismiss reason as `pfx_dialog`/`pkcs11_dialog`: a fresh
    /// "Choose signing certificate" or "Use card or token" attempt while this
    /// picker is still open must tear it down rather than stack a second
    /// dialog underneath it — see `sign::dismiss_sign_picker`.
    pub(crate) sign_picker_dialog: Option<Window>,
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

/// What kind of new page content a content-edit-mode click inserts, when
/// `ViewerState::content_insert_mode` is armed (T-163).
///
/// Mirrors `Tool`'s shape (a small `Copy` enum the toolbar arms one of at a
/// time) but needs none of `Tool`'s richer API — insertion has no drag-drawn
/// geometry, no markup/rule/freehand distinctions, just "what does the next
/// click create".
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum ContentInsertKind {
    Text,
    Image,
}

/// One form field kind the forms toolbar can place (T-141) — the field-kind
/// twin of `Tool`, minus everything about freehand strokes or markup that has
/// no meaning for a field.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum FieldKind {
    Text,
    Checkbox,
    RadioGroup,
    Dropdown,
}

impl FieldKind {
    pub(crate) const ALL: [FieldKind; 4] = [
        FieldKind::Text,
        FieldKind::Checkbox,
        FieldKind::RadioGroup,
        FieldKind::Dropdown,
    ];

    /// The kind's toolbar button label, also used in status messages.
    pub(crate) fn label(self) -> &'static str {
        match self {
            FieldKind::Text => "Text field",
            FieldKind::Checkbox => "Checkbox",
            FieldKind::RadioGroup => "Radio group",
            FieldKind::Dropdown => "Dropdown",
        }
    }

    /// The base a freshly placed field of this kind's `/T` name is derived
    /// from — `FormFieldSet::unique_name` appends the first free `_N` suffix.
    pub(crate) fn name_base(self) -> &'static str {
        match self {
            FieldKind::Text => "Text",
            FieldKind::Checkbox => "Checkbox",
            FieldKind::RadioGroup => "RadioGroup",
            FieldKind::Dropdown => "Dropdown",
        }
    }
}

/// The forms toolbar's controls (T-141): the mode toggle, the four placement
/// toggles paired with the kind each arms, and the style inspector for the
/// selected field.
#[derive(Clone)]
pub(crate) struct FormFieldToolbar {
    pub(crate) mode: ToggleButton,
    pub(crate) place: Vec<(FieldKind, ToggleButton)>,
    pub(crate) font: DropDown,
    pub(crate) size: SpinButton,
    pub(crate) color: Button,
    /// True while the inspector is being written from the selected field's
    /// style, so the controls' own change handlers know not to treat that
    /// write-back as a user edit and record a spurious restyle — same guard
    /// shape `tools_panel::build_tab_switcher` uses to keep its tab strip
    /// from driving the `Stack` it is only meant to follow.
    pub(crate) syncing: Rc<Cell<bool>>,
    /// Shown instead of `fill_rows` when the open document has no form
    /// fields (or none is open) — T-142's twin of `tools_panel`'s own
    /// "Comments aren't available" placeholder.
    pub(crate) fill_placeholder: Label,
    /// One row per form field, torn down and rebuilt by `forms::fill::refresh`
    /// on every document/selection/undo change — see that module's own doc
    /// for why a fill commit itself never triggers a rebuild.
    pub(crate) fill_rows: GtkBox,
    /// The control `forms::fill::focus_field` grabs keyboard focus on for a
    /// given field, rebuilt alongside `fill_rows` by `forms::fill::refresh`
    /// (T-143). Keyed separately from the row widgets themselves because a
    /// radio group's target is one of several buttons, not the row's own
    /// container.
    pub(crate) focus_targets: Rc<RefCell<HashMap<FormFieldId, gtk::Widget>>>,
}

/// The document-properties panel's widgets (T-176): the read-only page count
/// plus one `Entry` per `/Info` text field and per date field, kept in step
/// by `metadata::refresh`. Editing a field records `Command::SetDocumentInfo`
/// — see the `metadata` module doc for why the panel reads the log rather
/// than a value mirrored on `Document` itself.
#[derive(Clone)]
pub(crate) struct MetadataPanel {
    /// Guards against a programmatic `refresh` write-back being mistaken for
    /// a user edit by the entries' own `changed`/`activate` handlers — same
    /// shape as `FormFieldToolbar::syncing`.
    pub(crate) syncing: Rc<Cell<bool>>,
    pub(crate) pages: Label,
    pub(crate) title: Entry,
    pub(crate) author: Entry,
    pub(crate) subject: Entry,
    pub(crate) keywords: Entry,
    pub(crate) creator: Entry,
    pub(crate) producer: Entry,
    pub(crate) creation_date: Entry,
    pub(crate) mod_date: Entry,
    /// `creation_date`/`mod_date` show a friendly `YYYY-MM-DD HH:MM:SS` with
    /// no timezone of its own — these hold the UT offset the last-read
    /// `PdfDate` actually carried, so committing an edited date preserves it
    /// instead of silently resetting every edited date to UTC. Defaults to
    /// `Utc` for a field with no prior date to read one from — the same
    /// default `PdfDate::parse` itself falls back to for a wholly absent
    /// offset (PDF 32000-1:2008 §7.9.4).
    pub(crate) creation_offset: Rc<Cell<PdfDateOffset>>,
    pub(crate) mod_offset: Rc<Cell<PdfDateOffset>>,
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

/// The inline text editor open over a content-edit-mode click, if any.
///
/// Lives on the session rather than being committed immediately, the same
/// posture as [`Placement`]: nothing reaches the `EditLog` until the entry is
/// committed, and replacing the document drops a half-typed edit with it.
pub(crate) struct ContentEditor {
    pub(crate) page_index: usize,
    /// The run this editor is replacing, or — when [`Self::is_insertion`] is
    /// `true` — a template for the run it will insert: `page`/`bbox`/
    /// `resource_font_name`/`font_kind` are already the values the new run
    /// will carry, and `text` is the empty string until commit. Kept whole
    /// (not just an id) because `Command::ReplaceTextRunContent` is matched
    /// against the exact snapshot it was read from — see
    /// `pdf_document::content`'s module docs — and an insertion needs
    /// somewhere to hold the same fields before there is a run to match
    /// against at all.
    pub(crate) run: TextRun,
    pub(crate) entry: Entry,
    /// `true` when this editor is composing a brand-new run (T-163's "insert
    /// text" sub-mode) rather than retyping `run` in place. `commit` branches
    /// on this to call `pdf_edit::insert_text_run`/`Command::InsertTextRun`
    /// instead of the replace path — the empty-`run.text` template above is
    /// what makes the existing "no-op when nothing changed" check double as
    /// "close without recording an empty insertion".
    pub(crate) is_insertion: bool,
    /// The position in the document's `EditLog` of the command this edit must
    /// be folded into, when `run` already has one queued against it — an
    /// insertion made this session, or a replacement already recorded.
    ///
    /// `None` is the ordinary case: the run comes from the file as last
    /// saved, and committing records a new command. When it is `Some`,
    /// `commit` amends that entry instead, and takes precedence over
    /// `is_insertion` — see `content_edit::command::pending_text_command_index`
    /// for why a second command against one item could never resolve at save
    /// time.
    pub(crate) amends: Option<usize>,
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

/// The content-edit twin of [`AnnotationDrag`]: a selected image being moved
/// or resized right now, in content-edit mode (T-162).
///
/// Forked rather than shared with `AnnotationDrag` per design: images are
/// page content, not `document.annotations`, so unifying the two would blur
/// that boundary for no shared behavior beyond the shape.
pub(crate) struct ImageDrag {
    pub(crate) page_index: usize,
    /// The image this drag is moving or resizing. Held by value, the same
    /// reason [`ContentEditor`] holds a whole `TextRun`: page content has no
    /// document-wide id to re-fetch by, only the snapshot the parser handed
    /// back.
    pub(crate) item: ImageItem,
    pub(crate) mode: AnnotationDragMode,
    /// Where the pointer went down, in PDF page space.
    pub(crate) origin: (f64, f64),
    /// Where the pointer is now, in PDF page space.
    pub(crate) current: (f64, f64),
}

/// A text run being dragged across the page right now, in content-edit
/// mode — the text twin of [`ImageDrag`].
///
/// Simpler than its image sibling in one way and richer in another. There is
/// no `mode`: a run has no resize handles, because its width and height come
/// from its font rather than from a box anyone may pull on. But it carries
/// the reason it may not be moved, resolved once when the press lands (what
/// makes a run unmovable is a pending command, and the log cannot change
/// while the pointer is down) and reported only if the press turns out to be
/// a drag.
pub(crate) struct TextDrag {
    pub(crate) page_index: usize,
    /// The page as it is rendered right now, captured once when the press
    /// lands, so the drag can show the *text* moving rather than an empty
    /// box. `None` when the page's pixels could not be read — the outline
    /// still follows the pointer, which is the same feedback an image drag
    /// gives.
    pub(crate) preview: Option<DragPreview>,
    /// The run being moved. Held by value for the same reason
    /// [`ContentEditor`] holds one: page content has no document-wide id to
    /// re-fetch by, only the snapshot the parser handed back.
    pub(crate) run: TextRun,
    /// Why this run cannot be moved, if it cannot.
    pub(crate) refusal: Option<&'static str>,
    /// Where the pointer went down, in PDF page space.
    pub(crate) origin: (f64, f64),
    /// Where the pointer is now, in PDF page space.
    pub(crate) current: (f64, f64),
}

/// A copy of a page's rendered pixels, taken when a text drag begins.
///
/// Captured once rather than per frame: downloading a page texture is
/// megabytes of memcpy, and nothing about the page changes while the pointer
/// is down — only where the run is being carried to.
pub(crate) struct DragPreview {
    /// The page's rendered bitmap.
    pub(crate) page: cairo::ImageSurface,
    /// Bitmap pixels per widget unit, so the patch can be cut out of the
    /// bitmap using coordinates computed in the widget's own space.
    pub(crate) scale: f64,
    /// What to paint over the area the run is being carried away from,
    /// sampled from the page just outside the run's own box — a page is not
    /// necessarily white, and a white hole on a coloured one would read as
    /// damage rather than as a preview.
    pub(crate) background: (f64, f64, f64),
}

/// An image selected in content-edit mode, if any (T-162).
///
/// Like [`ContentEditor`], this holds the item by value rather than an id:
/// an `ImageItem` is not addressable page state, only a parser's snapshot of
/// one `Do` operator, so there is nothing to re-fetch by id once selected.
pub(crate) struct SelectedImage {
    pub(crate) page_index: usize,
    pub(crate) item: ImageItem,
}

/// A form field being moved or resized right now, in forms-edit mode
/// (T-141) — the forms twin of [`AnnotationDrag`].
///
/// Id-based, like `AnnotationDrag` and unlike `ImageDrag`/`TextDrag`: a form
/// field is addressable document state (`FormFieldSet::get`/`get_mut` by
/// `FormFieldId`), not a page-content snapshot with nothing to re-fetch by.
pub(crate) struct FormFieldDrag {
    pub(crate) id: FormFieldId,
    pub(crate) mode: AnnotationDragMode,
    /// Where the pointer went down, in PDF page space.
    pub(crate) origin: (f64, f64),
    /// Where the pointer is now, in PDF page space.
    pub(crate) current: (f64, f64),
}

/// A form field being placed on a page right now, between the pointer going
/// down and coming back up (T-141) — the forms twin of [`Placement`], minus
/// `tool`/`points`: a field is never freehand, so there is nothing to record
/// but the two corners of the drag.
pub(crate) struct FormPlacement {
    pub(crate) kind: FieldKind,
    pub(crate) page_index: usize,
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

/// Shown when the document allows content changes but this shell could not
/// build an editable model for it — the content-edit twin of
/// [`ANNOTATION_MODEL_UNAVAILABLE`], both driven by the same `document_model`.
pub(crate) const CONTENT_MODEL_UNAVAILABLE: &str =
    "This document could not be prepared for content changes.";

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

/// Whether this document's page content (text runs, images) may be edited —
/// the content-edit twin of [`AnnotationAccess`], read once at open time and
/// cached for the session.
///
/// A separate type rather than reusing `AnnotationAccess`: a document can
/// grant the annotate permission while withholding the modify-contents
/// permission, or vice versa (see `pdf_manip::content_editing_is_allowed`),
/// so the two must be able to disagree.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ContentEditAccess {
    /// Unencrypted, or the permissions (or an owner credential) allow it, and
    /// the editable model was built.
    Allowed,
    /// The document's `/P` withholds the modify-contents permission.
    Forbidden,
    /// The permission question could not be answered, or the editable model
    /// could not be built. Refused rather than assumed permissive, for the
    /// same reason as [`AnnotationAccess::Unavailable`].
    Unavailable,
}

impl ContentEditAccess {
    /// The message to show instead of editing content, or `None` when
    /// editing is allowed.
    pub(crate) fn refusal(self) -> Option<&'static str> {
        match self {
            ContentEditAccess::Allowed => None,
            ContentEditAccess::Forbidden => Some("This document does not permit content changes."),
            ContentEditAccess::Unavailable => Some(CONTENT_MODEL_UNAVAILABLE),
        }
    }
}

pub(crate) struct DocumentSession {
    pub(crate) document: DocumentHandle,
    /// Whether search and text selection may read this document's text.
    pub(crate) text_access: TextAccess,
    pub(crate) annotation_access: AnnotationAccess,
    pub(crate) content_edit_access: ContentEditAccess,
    /// The editable core model. Rendering remains backed by pdfium until a
    /// future save/reopen refresh, but every annotation command is recorded in
    /// this model's EditLog immediately.
    pub(crate) document_model: Option<Document>,
    pub(crate) save_backing: Option<SaveBacking>,
    /// Whether the in-memory model — and, since T-163, the pdfium handle
    /// currently rendering `document` — has diverged from whatever is on
    /// disk.
    ///
    /// Set by whatever *records* a command — `annotations::command::command`,
    /// `annotations::command::history`, and each content-edit commit site —
    /// never by the refresh that later catches the canvas up. That ordering
    /// is the whole point: `document::refresh_after_content_edit` runs a
    /// background save+reopen that can fail, and a document whose edit is
    /// already in the `EditLog` must report itself dirty even when the
    /// preview behind it never updated.
    ///
    /// Only `show_document`'s own default (`false`, a freshly opened document
    /// matches disk) and the real disk-save reopen (`document::spawn_save`,
    /// which also leaves it `false` — the file it just wrote *is* what is now
    /// shown) clear this. The preview refresh restores it to `true` across
    /// its own reopen (`document::restore_edit_state`), because the bytes it
    /// showed were never written anywhere.
    pub(crate) unsaved_to_disk: bool,
    pub(crate) edit_revision: u64,
    pub(crate) next_annotation_id: u64,
    pub(crate) selected_annotation: Option<AnnotationId>,
    /// Next id a placed form field gets (T-141). Unlike
    /// `next_annotation_id`, this cannot always start at 0: an opened PDF's
    /// own AcroForm fields are read into `document.form_fields` at open time
    /// with ids assigned sequentially from 0 (`pdf_form::read_form_fields`),
    /// so a freshly placed field's id must continue past whatever that read
    /// already claimed — see `document::next_form_field_id`.
    pub(crate) next_form_field_id: u64,
    pub(crate) selected_form_field: Option<FormFieldId>,
    /// The form field being placed right now, if a placement drag is in
    /// flight (T-141).
    pub(crate) form_placement: Option<FormPlacement>,
    /// The selected form field being moved or resized right now, if any
    /// (T-141).
    pub(crate) form_field_drag: Option<FormFieldDrag>,
    /// Decoded stamp images stay in the GTK session, never in the editable PDF
    /// model, so the pending-save representation remains toolkit-independent.
    ///
    /// Cairo surfaces rather than `gdk::Texture`: the draw function runs on
    /// every frame, and downloading a texture there would copy the whole
    /// bitmap out of the GPU per redraw — visible as stutter while dragging a
    /// screenshot-sized stamp. Decoding once on insert makes painting a blit.
    ///
    /// Entries are never removed. Deleting an annotation is undoable, so the
    /// surface has to outlive it; ids are allocated monotonically per session,
    /// so a stale entry can never be matched by a later annotation.
    pub(crate) stamp_surfaces: HashMap<AnnotationId, cairo::ImageSurface>,
    /// The annotation being drawn right now, if a placement drag is in flight.
    pub(crate) placement: Option<Placement>,
    /// The selected annotation being moved or resized right now, if any.
    pub(crate) annotation_drag: Option<AnnotationDrag>,
    /// The inline content editor open right now, if any. Lives on the session
    /// so replacing the document drops it with the run it addressed — same
    /// reasoning as `annotation_drag`.
    pub(crate) content_editor: Option<ContentEditor>,
    /// The selected image in content-edit mode, if any (T-162). Mutually
    /// exclusive with `content_editor` per design: at most one of a text-run
    /// editor and an image selection is open on the page at a time.
    pub(crate) selected_image: Option<SelectedImage>,
    /// The selected image being moved or resized right now, if a drag is in
    /// flight. Lives on the session for the same reason `annotation_drag`
    /// does: nothing reaches the `EditLog` until the pointer comes up.
    pub(crate) image_drag: Option<ImageDrag>,
    /// The text run being dragged across the page right now, if any. Same
    /// reasoning as `image_drag`, and mutually exclusive with it in
    /// practice: a press is claimed by at most one item.
    pub(crate) text_drag: Option<TextDrag>,
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
    /// This page's content-stream text runs and images, loaded on first entry
    /// into content-edit mode. Unlike `characters`, this is pure computation
    /// over the already-in-memory base document (no pdfium round trip), so it
    /// is filled in synchronously the first time it is needed — see
    /// `content_edit::model::ensure_page_content`.
    pub(crate) content: Option<PageContent>,
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
    pub(crate) content_edit_access: ContentEditAccess,
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
