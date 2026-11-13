//! The inline text editor F2 or a double click opens over a selected text
//! run or block in content-edit mode.
//!
//! First widget-over-page-coordinates pattern in this shell: annotations are
//! painted on the highlights `DrawingArea`, never real widgets, and the
//! password prompt is a modal top-level window rather than something
//! positioned against a page. This adds a real `gtk::Entry` as a third child
//! of the page's `Overlay`, positioned from `pdf_render::place_rect` — the
//! same "coordinates are pre-scaled in Rust" posture the rest of the shell
//! uses. A paragraph block gets a multi-line `TextView` in the entry's place
//! (`super::paragraph_editor`); everything else here is shared by both.
//!
//! The one exception to "no transform" is the page's own turn. A `/Rotate`
//! makes the run this box is editing run down the screen rather than across
//! it, and a box whose text stayed level would be a box that no longer looks
//! like the thing it is replacing. GTK4 can only turn a widget through a
//! `GtkFixed`'s child transform, so the entry sits inside a `Fixed` that is
//! itself placed the way the bare entry used to be — see [`place_entry`].
//!
//! Being a real widget is also what lets a *new* box be nudged into place
//! before anything is typed into it: the same margins that position it are
//! what [`wire_drag`] updates. That is the only drag this module owns —
//! moving a run that already exists belongs to the page gesture in
//! [`super::text`], which does not have to fight the text field for the
//! press.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gdk, glib, graphene, gsk, Entry, EventControllerFocus, EventControllerKey, EventSequenceState,
    Fixed, GestureDrag, PropagationPhase, ScrolledWindow,
};
use pdf_document::{Command, ContentItemId, FontKind, Rect, TextRun};
use pdf_edit::EditError;
use pdf_render::{place_point, point_to_pdf, PagePlacement, PageRotation};

use crate::app::i18n::tr;
use crate::app::state::{ContentEditor, DocumentSession, EditorField, Viewer};
use crate::app::update_content_edit_controls;
use crate::app::write::refresh_preview;

use super::camouflage::{self, Camouflage, EditorLook};
use super::command::{self, pending_move_index, pending_text_command, PendingText};
use super::model;
use super::paragraph_editor;
use super::CLICK_EPSILON_PX;

/// Fixed default box for a newly inserted text run (T-163), in PDF points —
/// there is no existing run to inherit a size from the way a replacement
/// does. 150pt wide is comfortably more than a short phrase in Helvetica at
/// this height; 14pt tall matches `insert_text_run`'s own reading of
/// `bbox.height` as the font size (`core/pdf-edit/src/insert.rs`), so 14pt
/// is both the box height and the point size the text is drawn at — large
/// enough to read at 100% zoom.
const INSERT_TEXT_WIDTH_PT: f64 = 150.0;
const INSERT_TEXT_HEIGHT_PT: f64 = 14.0;

/// Opens an inline editor over `run` on `page_index`.
///
/// Resolves whatever editor is already open first — a click always commits
/// the edit in progress before starting a new one, never abandons it
/// silently. Composite-font runs never open an editor at all: `pdf-edit`
/// rejects every replacement against one outright, so there is nothing an
/// editor here could do but fail; the refusal is reported immediately
/// instead, reusing `EditError`'s own message.
///
/// A run that already has an unsaved edit queued against it opens like any
/// other, prefilled with the text that edit gave it — the box on screen shows
/// what the page shows. What changes is where the commit lands: `amends`
/// carries the log entry to fold the result into, rather than a second
/// command being appended.
pub(crate) fn open_editor(viewer: &Viewer, page_index: usize, run: TextRun, block: Vec<TextRun>) {
    commit(viewer);
    if keep_refused_editor(viewer) {
        return;
    }

    // `run` stands in for `block` when there is one (B25 T-207): its box and
    // text are the block's, and every member must be retypeable.
    if let Some(run) = std::iter::once(&run)
        .chain(&block)
        .find(|run| run.font_kind == FontKind::EmbeddedComposite)
    {
        viewer.status.set_text(
            &EditError::CompositeFontNotEditable {
                resource_font_name: run.resource_font_name.clone(),
            }
            .to_string(),
        );
        return;
    }

    // `run` may have come from `model::overlay_pending_content` rather than
    // the base document — a run that only exists because of a pending,
    // unsaved insertion, or one already carrying a queued replacement. Either
    // way its next edit amends the command already describing it instead of
    // queueing a second one no save could resolve; see
    // `command::pending_text_command`.
    //
    // A block's runs are the core's to judge when its batch is recorded
    // (`pdf_save::record_content_batch`), so these checks are a single run's.
    //
    // A run moved this session is refused only where the core cannot
    // checkpoint: where it can, the retype is refused by the queue, written
    // into a new in-memory base with the move, and retried there
    // (`super::checkpoint`).
    let (pending, moved) = {
        let state = viewer.state.borrow();
        let session = state.session.as_ref();
        let document = session.and_then(|session| session.document_model.as_ref());
        match (session, document) {
            (Some(session), Some(document)) if block.is_empty() => (
                pending_text_command(document, &run),
                pending_move_index(document, &run).is_some()
                    && !super::checkpoint::may_checkpoint(session),
            ),
            _ => (PendingText::Nothing, false),
        }
    };
    let amends = match pending {
        // `pending_text_command` is deliberately blind to a pending
        // `MoveTextRun` (see its own doc — `text::plan_move` needs `Nothing`
        // here to amend a second move itself), so a run only moved this
        // session still has to be caught here before falling through to an
        // ordinary replacement: `commit`'s replace branch validates against
        // `base`, which the move never touched, so the run's *current*
        // (moved) bbox would fail to resolve there with an opaque "no
        // content item" — the retype and the move are not two operations
        // `pdf-edit` has a combined command for, unlike two retypes of the
        // same run.
        PendingText::Nothing if moved => {
            viewer.status.set_text(&tr("edit-text-moved-unsaved"));
            return;
        }
        PendingText::Nothing => None,
        PendingText::Amend(index) => Some(index),
        // A run the overlay showed against a log that no longer holds the
        // command behind it. There is nothing to amend and nothing in the
        // base document to target either, so opening an editor could only
        // produce a command `pdf-edit` fails to resolve at save time —
        // refused here, same posture as the composite-font case above.
        PendingText::Unresolvable => {
            viewer.status.set_text(&tr("edit-text-edit-unavailable"));
            return;
        }
    };

    // The editor takes over from a selected block (F2, a double click): at
    // most one of the two is ever live, so Delete and F2 stand down while
    // the box is open and its keys stay the box's. Only here, past every
    // refusal above, so a block that cannot be retyped stays selected.
    // A double click on text over an image has already selected the image
    // on its first press; that goes too, or Delete would remove the picture
    // instead of a character.
    super::block::clear(viewer);
    if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
        session.selected_image = None;
    }

    // A paragraph — a block of several lines — is retyped in a multi-line
    // field whose line breaks the retype keeps (`super::paragraph_editor`);
    // a run and a one-line block keep the one-line entry.
    let field = if model::is_paragraph(&block) {
        paragraph_editor::new_field(&run.text)
    } else {
        let entry = Entry::new();
        entry.set_text(&run.text);
        EditorField::Line(entry)
    };
    {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(page) = session.pages.get(page_index) else {
            return;
        };
        let placement = page.placement();

        // Drawn as the page draws the text it retypes (`super::camouflage`):
        // the run's font, size and colour over the page's own colour.
        let outer = field.outer_widget();
        let look = camouflage::look_for(
            session,
            page_index,
            &run,
            &block,
            &outer
                .pango_context()
                .font_map()
                .expect("a widget's font map"),
        );
        if let EditorField::Paragraph { view, .. } = &field {
            view.set_justification(look.justification());
        }
        let camouflage = Camouflage::apply(look, &outer, placement.scale);

        let frame = Fixed::new();
        frame.put(&outer, 0.0, 0.0);
        place_field(&frame, &field, run.bbox, placement, Some(&camouflage.look));
        let Some(page) = session.pages.get(page_index) else {
            return;
        };
        page.overlay.add_overlay(&frame);

        session.content_editor = Some(ContentEditor {
            page_index,
            run: run.clone(),
            field: field.clone(),
            frame,
            is_insertion: false,
            amends,
            block,
            camouflage: Some(camouflage),
        });
    }

    match &field {
        EditorField::Line(entry) => {
            wire_entry(viewer, entry, false);
            // A camouflaged line widens as it is typed past the run (`refit`).
            entry.connect_changed({
                let viewer = viewer.clone();
                move |_| refit(&viewer)
            });
            focus_without_scrolling(&viewer.scroll, entry);
            entry.select_region(0, -1);
        }
        // The view opens with its text selected (`new_field`).
        EditorField::Paragraph { view, .. } => {
            paragraph_editor::wire(viewer, view);
            focus_without_scrolling(&viewer.scroll, view);
        }
    }
    // The Edit page's "Delete text" is gated on exactly this editor being
    // open over an existing run, so the page learns about it here — the
    // image card's controls are refreshed the same way from every
    // select/deselect in `super::image`.
    update_content_edit_controls(viewer);
}

/// Opens a blank inline editor at `point` (PDF page space) to compose a
/// brand-new text run (T-163's "insert text" sub-mode), rather than
/// retyping an existing one.
///
/// Resolves whatever editor is already open first, same as [`open_editor`]:
/// a click always commits the edit in progress before starting a new one.
/// Unlike a replacement there is no run to read a font/size/position from,
/// so this picks a fixed default box anchored at `point` and a font
/// resource name guaranteed not to collide with one already on the page —
/// see [`super::model::unused_font_resource_name`]'s own doc for why a
/// colliding name would be a silent miscoding bug, not just a cosmetic one.
pub(crate) fn open_insert_editor(viewer: &Viewer, page_index: usize, point: (f64, f64)) {
    commit(viewer);
    if keep_refused_editor(viewer) {
        return;
    }
    super::block::clear(viewer);

    let entry = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        let Some(base) = session.save_backing.as_ref().map(|backing| &backing.base) else {
            return;
        };
        let Some(document) = session.document_model.as_ref() else {
            return;
        };
        // Collected into an owned `Vec` before `session.pages` is borrowed
        // mutably below — see `image::apply_insertion` for the same shape and
        // the reason the base document alone cannot answer this.
        let reserved = model::reserved_font_resource_names(&document.pending_edits);
        let pending = Some(&document.pending_edits);
        // The canvas index names a page id only through the open handle, and
        // only a page whose bytes this session can reach has content to
        // parse — see `super::content_page`.
        let Some(page_id) = super::content_page(session, page_index) else {
            return;
        };
        // Before the mutable `pages` borrow below: the probe reads other
        // fields of the same session.
        let Some(probe) = super::page_probe(document, base, &session.imported_sources, page_id)
        else {
            return;
        };
        let Some(page) = session.pages.get_mut(page_index) else {
            return;
        };
        let resource_font_name =
            match model::ensure_page_content(&mut page.content, probe, page_id, pending) {
                Ok(content) => model::unused_font_resource_name(content, &reserved),
                Err(error) => {
                    drop(state);
                    viewer.status.set_text(&error.to_string());
                    return;
                }
            };

        // The box's bottom-left sits at `point`, matching how
        // `pdf_edit::insert::insert_text_run` reads the bbox back: the
        // baseline it computes (`bbox.y + 0.25 * size`) sits just above the
        // box's own bottom edge, so what the user clicked is where the text
        // actually lands, not somewhere inside an invisible margin.
        let bbox = Rect {
            x: point.0,
            y: point.1,
            width: INSERT_TEXT_WIDTH_PT,
            height: INSERT_TEXT_HEIGHT_PT,
        };
        let run = TextRun {
            // Never consulted by `pdf_edit::insert_text_run` — insertion has
            // no existing item to target, so this id is a placeholder only;
            // `PageContent`'s real ids come from the *next* parse, once the
            // run this template describes actually exists on the page.
            id: ContentItemId(0),
            page: page_id,
            bbox,
            resource_font_name,
            font_kind: FontKind::Standard14,
            text: String::new(),
        };

        let entry = Entry::new();
        let frame = Fixed::new();
        frame.put(&entry, 0.0, 0.0);
        place_entry(&frame, &entry, bbox, page.placement());
        page.overlay.add_overlay(&frame);

        session.content_editor = Some(ContentEditor {
            page_index,
            run,
            field: EditorField::Line(entry.clone()),
            frame,
            is_insertion: true,
            block: Vec::new(),
            // Nothing to amend: this run has no command of its own yet. The
            // one this commit records is what a *later* click on the same
            // text will amend, via the synthetic id the overlay mints for it.
            amends: None,
            camouflage: None,
        });

        entry
    };

    wire_entry(viewer, &entry, true);
    focus_without_scrolling(&viewer.scroll, &entry);
    // Same call as [`open_editor`]'s, for the opposite outcome: an insertion
    // has no run on the page yet, so this is what keeps "Delete text"
    // *disabled* while a blank box is open.
    update_content_edit_controls(viewer);
}

/// Puts the cursor in `entry` without letting the page canvas jump.
///
/// A box is always opened where the user just clicked, so it is on screen by
/// construction and there is never anything for a scroll to reveal. But
/// `grab_focus` on a child that has only just been added is a child the
/// `ScrolledWindow` has not measured yet, and it scrolls to the origin to
/// bring into view a widget it believes is sitting there. The reader is thrown
/// back to the start of the page and the run they were editing leaves the
/// screen.
///
/// Measured before the fix: canvas parked at 100, a run whose box computes to
/// x=144 in a 516-wide viewport — fully visible — and opening its editor left
/// the scroll at 0. Only the first focus does it; once the entry has been
/// allocated, focusing it again moves nothing.
///
/// This predates any of the rotation work and was simply never visible: with
/// the default fit there is no horizontal scroll to lose. A page turned to
/// landscape is the first thing that gives the canvas somewhere to jump from.
/// Whether an editor is still open after [`commit`] — its text was refused
/// (a character the run's font cannot encode, say) — in which case the box
/// takes the cursor back and the caller must not start anything new.
///
/// Every caller that commits and then claims a click asks this. Opening a
/// second editor over the refused one replaced `content_editor` while the
/// first box stayed on the page with its focus-out handler still wired: the
/// focus moving into the new box fired `commit` against the *new* editor,
/// which tore it down mid-focus-change and sent GTK into an endless stream
/// of `gtk_widget_get_parent` criticals until the process died. The refusal
/// is already on the status line; the user fixes the text or presses Escape.
pub(crate) fn keep_refused_editor(viewer: &Viewer) -> bool {
    let field = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.content_editor.as_ref())
        .map(|editor| editor.field.focus_widget());
    let Some(field) = field else {
        return false;
    };
    focus_without_scrolling(&viewer.scroll, &field);
    true
}

fn focus_without_scrolling(scroll: &ScrolledWindow, field: &impl IsA<gtk::Widget>) {
    let (horizontal, vertical) = (scroll.hadjustment(), scroll.vadjustment());
    let (left, top) = (horizontal.value(), vertical.value());
    field.grab_focus();
    horizontal.set_value(left);
    vertical.set_value(top);
}

/// Positions the editor box over `bbox` on `page`, in the page overlay's
/// own coordinates.
///
/// The one place the widget's geometry is computed, shared by opening an
/// editor, opening a blank one for an insertion, and putting the box back
/// after a refused drag — three call sites that must agree on where a given
/// run's box sits, or a rejected move would leave the box somewhere the run
/// is not.
fn place_entry(frame: &Fixed, entry: &Entry, bbox: Rect, page: PagePlacement) {
    let size = entry_size(entry, bbox, page);
    place_sized(frame, entry, size, bbox, page);
}

/// [`place_entry`] for either kind of field, drawn in `look` when the
/// editor retypes text already on the page: then the box is the one
/// `super::camouflage::box_geometry` puts the first baseline on the page's
/// with, rather than `bbox` itself. A paragraph's view is sized by
/// `super::paragraph_editor`; both are placed — and turned — the same way.
fn place_field(
    frame: &Fixed,
    field: &EditorField,
    bbox: Rect,
    page: PagePlacement,
    look: Option<&EditorLook>,
) {
    match (field, look) {
        (EditorField::Line(entry), None) => place_entry(frame, entry, bbox, page),
        (EditorField::Line(entry), Some(look)) => {
            let (placed, size) = camouflaged_entry_size(entry, look, page);
            place_sized(frame, entry, size, placed, page);
        }
        (EditorField::Paragraph { view, scroller }, _) => {
            let (placed, size) = paragraph_editor::field_size(view, scroller, bbox, page, look);
            place_sized(frame, scroller, size, placed, page);
        }
    }
}

/// [`entry_size`] for an entry drawn in `look`: one line of its font tall,
/// its top where the font's ascent puts the baseline on the run's, and as
/// wide as the run — or as the typed text, when that is wider, so the line
/// grows to the right as the page will show it instead of scrolling inside
/// a box the run's width ([`refit`] re-places it as it changes). Read back
/// like [`entry_size`] in case GTK still will not go that small; growing
/// rightwards and downwards keeps the baseline put.
fn camouflaged_entry_size(
    entry: &Entry,
    look: &EditorLook,
    page: PagePlacement,
) -> (Rect, (f64, f64)) {
    let metrics = camouflage::LineMetrics::of(&entry.pango_context(), look, page.scale);
    let layout = entry.create_pango_layout(Some(&entry.text()));
    layout.set_font_description(Some(&look.font_description(page.scale)));
    layout.set_attributes(Some(&look.attributes(page.scale)));
    let (text_width, _) = layout.pixel_size();
    let geometry = camouflage::box_geometry(
        look,
        page.scale,
        metrics,
        metrics.ascent_px + metrics.descent_px,
    )
    .grown_to(
        camouflage::line_width_px(f64::from(text_width)),
        0.0,
        page.scale,
    );
    entry.set_width_request(geometry.width_px as i32);
    entry.set_height_request(geometry.height_px as i32);
    entry.set_max_width_chars(1);
    let (minimum, _) = entry.preferred_size();
    let geometry = geometry.grown_to(
        f64::from(minimum.width()),
        f64::from(minimum.height()),
        page.scale,
    );
    entry.set_width_request(geometry.width_px as i32);
    entry.set_height_request(geometry.height_px as i32);
    (geometry.placed, (geometry.width_px, geometry.height_px))
}

/// Positions `child`, already `(width, height)` in its own upright frame,
/// over `bbox` — the arithmetic [`place_entry`] and [`place_field`] share.
fn place_sized(
    frame: &Fixed,
    child: &impl IsA<gtk::Widget>,
    (width, height): (f64, f64),
    bbox: Rect,
    page: PagePlacement,
) {
    // Where the run's PDF-space top-left corner lands on screen. The entry's
    // own top-left starts there whichever way the page is turned; the turn
    // decides which way the rest of the box then runs.
    let anchor = place_point((bbox.x, bbox.y + bbox.height), page);
    // The entry's box in the frame's coordinates. A quarter turn sends the
    // entry's height into *negative* screen x and the half turn sends both
    // sides negative, so the frame's origin is not the anchor: it is whichever
    // corner of the turned box comes out top-left. `local` is the anchor's
    // offset from it, and is exactly what the transform has to translate by.
    let (local_x, local_y) = match page.rotation {
        PageRotation::None => (0.0, 0.0),
        PageRotation::Clockwise90 => (height, 0.0),
        PageRotation::Clockwise180 => (width, height),
        PageRotation::Clockwise270 => (0.0, width),
    };

    frame.set_halign(gtk::Align::Start);
    frame.set_valign(gtk::Align::Start);
    // Clamped at zero because GTK refuses a negative margin, which is also
    // what stops a box from being carried off the top-left of the page.
    frame.set_margin_start((anchor.0 - local_x).round().max(0.0) as i32);
    frame.set_margin_top((anchor.1 - local_y).round().max(0.0) as i32);

    frame.set_child_transform(
        child,
        Some(
            &gsk::Transform::new()
                .translate(&graphene::Point::new(local_x as f32, local_y as f32))
                .rotate(page.rotation.degrees() as f32),
        ),
    );
}

/// The size the entry will actually occupy, in its own upright frame.
///
/// The run's box at this zoom is only a floor. A 14pt line is ten-odd pixels
/// tall at a readable zoom and GTK will not shrink an `Entry` below the height
/// of its own font, so asking for ten and getting thirty-four is the normal
/// case, not an edge one.
///
/// As a bare overlay child that overflow was invisible — the entry simply drew
/// a little taller than the run. Inside a `Fixed` it is not: the frame measures
/// the *transformed* bounds of its child and clamps them at its own origin, so
/// on a quarter turn an entry taller than the run asked for runs off the
/// frame's left edge and everything past it is clipped away. That is a box with
/// no visible text in it. Asking the entry what it actually needs, and then
/// requesting exactly that, is what keeps the frame, the transform and the
/// widget describing one box instead of three.
fn entry_size(entry: &Entry, bbox: Rect, page: PagePlacement) -> (f64, f64) {
    let requested = |points: f64| (points * page.scale).round().max(1.0) as i32;
    entry.set_width_request(requested(bbox.width));
    entry.set_height_request(requested(bbox.height));
    // Pins the entry's natural width to its minimum. Without it the natural
    // grows with the text, and `GtkFixed` measures a child by its natural: the
    // frame would quietly widen as the user typed while the transform below
    // went on translating by the size the box had when it opened, walking the
    // text off the frame's edge on a quarter turn. It also makes the box match
    // the run it is replacing rather than the sentence being typed into it —
    // the entry scrolls its own content, the way any entry too narrow for its
    // text does.
    entry.set_max_width_chars(1);
    // Read back rather than computed: the floor is GTK's — font metrics,
    // padding and whatever the theme's CSS says an entry may not be smaller
    // than — and none of that is ours to predict.
    let (minimum, _) = entry.preferred_size();
    // Re-requested so the minimum *is* the size: `GtkFixed` allocates a child
    // its preferred size, and a request equal to the minimum leaves no room
    // for the two to disagree.
    entry.set_width_request(minimum.width());
    entry.set_height_request(minimum.height());
    (f64::from(minimum.width()), f64::from(minimum.height()))
}

/// Makes an insertion's box draggable, so a new text box can be nudged into
/// place before anything is typed into it.
///
/// **Only an insertion's box.** An editor opened over an existing run does
/// not get this, deliberately: dragging inside a text field is how anyone
/// selects text with a mouse, and a gesture that stole it would trade an
/// everyday interaction for one the page already offers — pressing the run
/// itself and dragging it (`super::text::begin_text_drag`). A blank box has
/// no text to select yet, so there is nothing to trade.
///
/// Press and pull moves the box; press and release places the caret. Both
/// live in the same pixels and distance tells them apart, using the same
/// [`CLICK_EPSILON_PX`] threshold the page gesture uses. The controller runs
/// in the **capture** phase, ahead of the `Entry`'s own text gesture, and
/// claims the sequence only once the pointer has actually travelled.
fn wire_drag(viewer: &Viewer, entry: &Entry) {
    let drag = GestureDrag::new();
    drag.set_propagation_phase(PropagationPhase::Capture);

    // The run's PDF-space box when the press landed, and the page it is being
    // dragged on. `None` means this press must not move anything, either
    // because there is no editor to move or because moving this particular
    // run was refused before the drag could start.
    //
    // The *box*, not the widget's margins. Screen pixels stopped being a
    // usable record of where the box is the moment a page could be turned
    // under it: on a quarter turn a pointer moving right moves the run *down*
    // the page, and a margin cannot say that. Keeping the PDF box as the truth
    // and re-placing the widget from it makes the move the user watches and
    // the move that gets recorded one piece of arithmetic instead of two that
    // have to be kept in agreement.
    let anchor: Rc<Cell<Option<(Rect, PagePlacement)>>> = Rc::new(Cell::new(None));
    // Whether the pointer has passed the threshold, i.e. whether this gesture
    // has become a move rather than a click.
    let moving = Rc::new(Cell::new(false));

    drag.connect_drag_begin({
        let viewer = viewer.clone();
        let anchor = anchor.clone();
        let moving = moving.clone();
        move |_, _, _| {
            moving.set(false);
            anchor.set(dragged_from(&viewer));
        }
    });

    drag.connect_drag_update({
        let viewer = viewer.clone();
        let anchor = anchor.clone();
        let moving = moving.clone();
        move |gesture, offset_x, offset_y| {
            let Some((origin, page)) = anchor.get() else {
                return;
            };
            if !moving.get() {
                if offset_x.abs() < CLICK_EPSILON_PX && offset_y.abs() < CLICK_EPSILON_PX {
                    return;
                }
                // Past the threshold, so this is a move. Claiming the
                // sequence is what stops the `Entry` from going on selecting
                // text under the pointer for the rest of the drag.
                gesture.set_state(EventSequenceState::Claimed);
                moving.set(true);
            }
            reposition(&viewer, dragged_box(origin, page, offset_x, offset_y));
        }
    });

    drag.connect_drag_end({
        let viewer = viewer.clone();
        move |_, offset_x, offset_y| {
            let Some((origin, page)) = anchor.take() else {
                return;
            };
            if !moving.replace(false) {
                return;
            }
            // The same box the last `drag_update` put on screen, from the same
            // inputs — so what gets recorded is what the user watched happen,
            // with no need to read the widget back to find out what that was.
            finish_drag(&viewer, dragged_box(origin, page, offset_x, offset_y));
        }
    });

    entry.add_controller(drag);
}

/// The box a drag starting now would move, and the page it sits on — or
/// `None` when this press must not move anything.
fn dragged_from(viewer: &Viewer) -> Option<(Rect, PagePlacement)> {
    if !is_composing_an_insertion(viewer) {
        return None;
    }
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    let editor = session.content_editor.as_ref()?;
    let page = session.pages.get(editor.page_index)?;
    Some((editor.run.bbox, page.placement()))
}

/// `origin` moved by a pointer offset, in PDF space, clamped to the page.
///
/// The offset is turned back into PDF space by running both of its ends
/// through [`point_to_pdf`] and subtracting, rather than by a formula of its
/// own: the transform is affine, so the difference of the two *is* the
/// offset's PDF-space twin — turn and zoom included — and there is no second
/// derivation of the turn here to drift from the first.
fn dragged_box(origin: Rect, page: PagePlacement, offset_x: f64, offset_y: f64) -> Rect {
    let (from_x, from_y) = point_to_pdf(0.0, 0.0, page);
    let (to_x, to_y) = point_to_pdf(offset_x, offset_y, page);
    let moved = Rect {
        x: origin.x + f64::from(to_x - from_x),
        y: origin.y + f64::from(to_y - from_y),
        ..origin
    };
    // Kept on the page, the way the old margin clamp kept the widget off the
    // overlay's top-left corner — but in PDF space, where "the page" means the
    // same thing at every turn, and against all four edges rather than two.
    let (page_width, page_height) = page.unrotated();
    Rect {
        x: moved.x.clamp(0.0, (page_width - moved.width).max(0.0)),
        y: moved.y.clamp(0.0, (page_height - moved.height).max(0.0)),
        ..moved
    }
}

/// Moves the open editor's box to `bbox` on screen. Live drag feedback only —
/// nothing here reaches the run or the `EditLog`.
fn reposition(viewer: &Viewer, bbox: Rect) {
    // Read out and dropped before any widget is touched: placing the frame
    // relayouts the page, and a draw func reached that way takes its own
    // borrow of this same state.
    let editor = {
        let state = viewer.state.borrow();
        state.session.as_ref().and_then(|session| {
            let editor = session.content_editor.as_ref()?;
            let page = session.pages.get(editor.page_index)?.placement();
            let camouflage = editor.camouflage.as_ref();
            Some((
                editor.frame.clone(),
                editor.field.clone(),
                page,
                camouflage.map(|camouflage| camouflage.look.clone()),
                camouflage.and_then(|camouflage| camouflage.restyle_at(page.scale)),
            ))
        })
    };
    if let Some((frame, field, page, look, restyle)) = editor {
        if let Some((provider, css)) = restyle {
            provider.load_from_data(&css);
        }
        place_field(&frame, &field, bbox, page, look.as_ref());
    }
}

/// Re-fits the open editor's box to what it now holds — a paragraph grows
/// downwards as lines are typed into it, a camouflaged line rightwards — so
/// the box keeps covering the text rather than scrolling it out of sight.
pub(super) fn refit(viewer: &Viewer) {
    let bbox = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.content_editor.as_ref())
        .map(|editor| editor.run.bbox);
    if let Some(bbox) = bbox {
        reposition(viewer, bbox);
    }
}

/// Whether the open editor is composing a run that exists nowhere yet — the
/// one state in which the box itself may be dragged.
///
/// `amends` being set means the run is already in the log (and, for a
/// replacement, in the file), so moving it is a real edit that belongs to the
/// page gesture, not to this widget.
fn is_composing_an_insertion(viewer: &Viewer) -> bool {
    let state = viewer.state.borrow();
    state
        .session
        .as_ref()
        .and_then(|session| session.content_editor.as_ref())
        .is_some_and(|editor| editor.is_insertion && editor.amends.is_none())
}

/// Resolves a finished drag of an insertion's box, `dx`/`dy` being how far it
/// actually moved in device pixels.
///
/// Nothing reaches the `EditLog`: the run this box will insert does not exist
/// yet, so moving the box is not an edit of the page, only a different
/// description of what the eventual commit will add. That is what makes
/// "click, nudge into place, type" one uninterrupted gesture rather than
/// three edits.
fn finish_drag(viewer: &Viewer, destination: Rect) {
    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return;
    };
    let Some(editor) = session.content_editor.as_mut() else {
        return;
    };
    // Only the origin ever moves: the box keeps the size it was composed
    // with, and `dragged_box` carries `width`/`height` through untouched.
    editor.run.bbox = destination;
}

fn wire_entry(viewer: &Viewer, entry: &Entry, draggable: bool) {
    if draggable {
        wire_drag(viewer, entry);
    }

    entry.connect_activate({
        let viewer = viewer.clone();
        move |_| commit(&viewer)
    });

    let focus = EventControllerFocus::new();
    focus.connect_leave({
        let viewer = viewer.clone();
        // Weak: the controller lives on this very widget.
        let entry = entry.upcast_ref::<gtk::Widget>().downgrade();
        move |_| commit_on_focus_leave(&viewer, entry.clone())
    });
    entry.add_controller(focus);

    let keys = EventControllerKey::new();
    // Capture phase: `Entry`'s own internal key controller (bubble phase, by
    // default) sees Escape first otherwise and consumes it — the box's
    // selected text (`select_region` in `open_editor`) means GTK's default
    // handling has something to do with it, so it never reaches this
    // controller and `cancel` never fires.
    keys.set_propagation_phase(PropagationPhase::Capture);
    keys.connect_key_pressed({
        let viewer = viewer.clone();
        move |_, keyval, _keycode, _state| {
            if keyval == gdk::Key::Escape {
                cancel(&viewer);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    });
    entry.add_controller(keys);
}

/// Validates and records the open editor's text, then closes it.
///
/// Three shapes, in the order they are tried: amending the command a run
/// already has queued (`ContentEditor::amends`), inserting a brand-new run
/// (`is_insertion`), or replacing an untouched one. All three validate
/// against the same base document — the one the whole log was recorded
/// against — before anything reaches the log.
///
/// A no-op text (`after == run.text`) closes without touching the `EditLog`
/// at all: for a replacement that means retyping the same words is not an
/// edit, and for an insertion (`is_insertion`, T-163) `run.text` starts as
/// the empty string, so leaving the box empty is the same "nothing to
/// record" case rather than a special one. A failed validation leaves the
/// editor open with the user's text intact either way: see
/// `command::validate_replacement`/`validate_insert_text`'s docs for why this
/// is checked before the command is recorded rather than only at save time.
/// Safe to call with no editor open (focus-out and Enter both route here).
pub(crate) fn commit(viewer: &Viewer) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(&refusal);
        return;
    }

    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return;
    };
    let Some(editor) = session.content_editor.as_ref() else {
        return;
    };
    let after = editor.field.text();

    if after == editor.run.text {
        let editor = session.content_editor.take().expect("checked above");
        drop(state);
        detach(viewer, &editor);
        return;
    }

    let is_insertion = editor.is_insertion;
    let run = editor.run.clone();
    let block = editor.block.clone();

    // Recorded through the core's queue policy (`pdf_save::record`), which
    // decides where the retype goes: a run the overlay shows because of a
    // pending insertion or retype is folded into that entry, keeping the
    // snapshot the file holds, and a block is one batch (B25 T-207) — a
    // paragraph's wrapped to its width, line breaks kept.
    let (recorded, message) = if !block.is_empty() {
        (
            command::record_block_retype(session, &block, &after),
            tr("edit-text-updated"),
        )
    } else if is_insertion {
        (
            command::record(
                session,
                Command::InsertTextRun(TextRun { text: after, ..run }),
            ),
            tr("edit-text-inserted"),
        )
    } else {
        (
            command::record(session, Command::ReplaceTextRunContent { item: run, after }),
            tr("edit-text-updated"),
        )
    };

    match recorded {
        Ok(()) => {
            let editor = session.content_editor.take().expect("checked above");
            drop(state);
            detach(viewer, &editor);
            refresh_preview(viewer, message);
        }
        // The editor stays open holding what was typed, and says why.
        Err(error) => {
            drop(state);
            viewer.status.set_text(&error);
        }
    }
}

/// Why a run that only exists as an unsaved insertion cannot be deleted
/// from here, said in terms of what to do instead — only where the session
/// may not checkpoint (see [`removable`]).
///
/// A removal of such a run names a run no file contains yet. Where the core
/// may checkpoint, it writes the queue into a new in-memory base and removes
/// the run from there (`pdf_save::checkpoint::record`). Where it may not —
/// an encrypted or signed file, imported PDFs, page changes — undo removes
/// the insertion outright, which is the operation actually being asked for.
pub(crate) const INSERTION_NOT_DELETABLE: &str = "edit-insertion-not-deletable";

/// Removes the run the open inline editor sits on — or its whole block, as
/// one undo step (B25 T-207) — the Edit page's "Delete text".
///
/// Whatever is in the box is discarded rather than committed first: a delete
/// supersedes a retype of the same run. Where the removal lands is the
/// core's to decide (`pdf_save::record`): a run with a queued retype has that
/// entry amended into the removal, keeping the snapshot the file holds, and
/// a run that is only an unsaved insertion is removed through a checkpoint
/// — or refused where none may be taken, see [`INSERTION_NOT_DELETABLE`].
pub(crate) fn delete_open_run(viewer: &Viewer) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(&refusal);
        return;
    }

    let mut state = viewer.state.borrow_mut();
    let Some(session) = state.session.as_mut() else {
        return;
    };
    let Some(editor) = session.content_editor.as_ref() else {
        return;
    };
    if editor.is_insertion {
        return;
    }
    let run = editor.run.clone();
    let block = editor.block.clone();

    match record_removal(session, run, &block) {
        Ok(()) => {
            let editor = session.content_editor.take().expect("checked above");
            drop(state);
            detach(viewer, &editor);
            refresh_preview(viewer, tr("edit-text-deleted"));
        }
        Err(error) => {
            drop(state);
            viewer.status.set_text(&error);
        }
    }
}

/// Whether [`record_removal`] would take `run` (or its `block`) to the
/// core rather than refuse it outright — what the quick actions' and the
/// Edit card's Delete read, so neither offers a removal that is refused.
///
/// Only a lone run that exists as an unsaved insertion is in question, and
/// it is removable when the session may checkpoint. The checkpoint check
/// parses the page tree, so it is asked for that run alone.
pub(crate) fn removable(session: &DocumentSession, run: &TextRun, block: &[TextRun]) -> bool {
    !block.is_empty()
        || model::pending_log_index(run.id).is_none()
        || super::checkpoint::may_checkpoint(session)
}

/// Records the removal of `run` — or of its whole `block`, as one batch —
/// through the core's queue policy. Shared by the open editor's delete and
/// a selected block's (`super::block::delete_selected`), so the two refuse
/// exactly the same things, [`INSERTION_NOT_DELETABLE`] among them.
pub(super) fn record_removal(
    session: &mut DocumentSession,
    run: TextRun,
    block: &[TextRun],
) -> Result<(), String> {
    if !removable(session, &run, block) {
        return Err(tr(INSERTION_NOT_DELETABLE));
    }
    if block.is_empty() {
        command::record(session, Command::RemoveTextRun(run))
    } else {
        command::record_batch(session, pdf_edit::remove_block(block))
    }
}

/// Discards the open editor without recording anything (Escape, or content
/// edit mode being switched off while one is open).
pub(crate) fn cancel(viewer: &Viewer) {
    let editor = {
        let mut state = viewer.state.borrow_mut();
        state
            .session
            .as_mut()
            .and_then(|session| session.content_editor.take())
    };
    if let Some(editor) = editor {
        detach(viewer, &editor);
        viewer.status.set_text(&tr("edit-cancelled"));
    }
}

/// Commits the editor whose `field` just lost the keyboard focus, once GTK
/// has finished moving the focus away from it.
///
/// Focus-out is emitted from inside `synthesize_focus_change_events`, while
/// GTK is still walking the old focus widget's ancestors. A commit closes the
/// editor, and closing it takes the frame off the page overlay: the chain
/// from the old focus up to the window is cut mid-walk, so GTK's
/// `for (w = old_focus; w != ancestor; w = gtk_widget_get_parent (w))` never
/// meets the ancestor and spins on `gtk_widget_get_parent (NULL)` forever —
/// the frozen window and the endless `GTK_IS_WIDGET` criticals a click on any
/// focus-taking button (a panel toggle, say) caused while the box was open.
///
/// So the commit runs from a high-priority idle — after the focus change,
/// still ahead of the release half of the click that moved the focus. By
/// then the editor may already be gone (committed by a canvas click, which
/// then dropped the cursor here) or replaced by another one, so it only
/// commits while `field` is still the open editor's.
pub(super) fn commit_on_focus_leave(viewer: &Viewer, field: glib::WeakRef<gtk::Widget>) {
    let viewer = viewer.clone();
    glib::idle_add_local_full(glib::Priority::HIGH, move || {
        let open = field.upgrade().is_some_and(|field| {
            viewer
                .state
                .borrow()
                .session
                .as_ref()
                .and_then(|session| session.content_editor.as_ref())
                .is_some_and(|editor| editor.field.focus_widget() == field)
        });
        if open {
            commit(&viewer);
        }
        glib::ControlFlow::Break
    });
}

/// Takes the editor box off the page.
///
/// The single door every editor teardown goes through — commit, cancel, and
/// [`delete_open_run`] all end here — which is why the Edit page's controls
/// are refreshed from this one place rather than from each of the branches
/// above: "Delete text" is gated on an editor being open, and an editor that
/// closes without the page hearing about it leaves a live button aimed at a
/// run nothing is editing any more.
///
/// Always called with the `state` borrow already dropped (the callers do so
/// before detaching), which is what lets both this function and
/// `update_content_edit_controls` take their own.
fn detach(viewer: &Viewer, editor: &ContentEditor) {
    // Before the borrow: dropping the cursor fires the box's focus-out,
    // which routes to `commit` and borrows the state itself. The editor is
    // already off the session by now, so that commit finds nothing to do.
    release_focus(&editor.frame);
    {
        let state = viewer.state.borrow();
        if let Some(page) = state
            .session
            .as_ref()
            .and_then(|session| session.pages.get(editor.page_index))
        {
            page.overlay.remove_overlay(&editor.frame);
        }
    }
    update_content_edit_controls(viewer);
}

/// Takes the cursor out of `frame` when it is in there, leaving the window
/// with no focus widget at all.
///
/// Removing the widget that has the cursor makes GTK move it on by itself, to
/// the next focusable widget it finds, and the canvas's viewport answers
/// that focus change with an eased scroll of its own (its `scroll-to-focus`;
/// turning that off stops it). That is
/// what slid a zoomed-in page sideways on every commit, click-away or
/// Escape. With the focus cleared first there is nothing for GTK to move and
/// nothing for the viewport to chase; the next click or Tab starts afresh.
fn release_focus(frame: &Fixed) {
    let Some(root) = frame.root() else {
        return;
    };
    if root
        .focus()
        .is_some_and(|focused| focused.is_ancestor(frame))
    {
        root.set_focus(None::<&gtk::Widget>);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::{Overlay, Picture, Window};
    use pdf_render::PageRotation;

    /// A letter page turned `rotation`, drawn 1:1. The drawn size is already
    /// swapped for a quarter turn, the way pdfium reports it.
    fn turned(rotation: PageRotation) -> PagePlacement {
        let (width_pt, height_pt) = match rotation {
            PageRotation::Clockwise90 | PageRotation::Clockwise270 => (792.0, 612.0),
            _ => (612.0, 792.0),
        };
        PagePlacement {
            width_pt,
            height_pt,
            rotation,
            scale: 1.0,
        }
    }

    fn a_box() -> Rect {
        Rect {
            x: 100.0,
            y: 500.0,
            width: 150.0,
            height: 14.0,
        }
    }

    /// The whole reason the drag stopped speaking in margins. Dragging the box
    /// to the right is "right" on the *screen*, and on a turned page the
    /// screen's right is not the page's. Turn a page 90 degrees clockwise and
    /// the direction that used to be up now points right, so a rightward drag
    /// walks the run *up* its own page — `y` rising, since PDF y grows
    /// upwards. Three quarters of a turn puts up on the left, so the same
    /// drag walks it down.
    #[test]
    fn a_rightward_pointer_drag_moves_the_box_the_way_the_page_faces() {
        let origin = a_box();
        let moved = |rotation| dragged_box(origin, turned(rotation), 40.0, 0.0);

        let upright = moved(PageRotation::None);
        assert_eq!(
            (upright.x, upright.y),
            (140.0, 500.0),
            "along the page's +x"
        );

        let quarter = moved(PageRotation::Clockwise90);
        assert_eq!((quarter.x, quarter.y), (100.0, 540.0), "up the page");

        let half = moved(PageRotation::Clockwise180);
        assert_eq!((half.x, half.y), (60.0, 500.0), "along the page's -x");

        let three_quarters = moved(PageRotation::Clockwise270);
        assert_eq!(
            (three_quarters.x, three_quarters.y),
            (100.0, 460.0),
            "down the page"
        );
    }

    /// The box keeps the size it was composed with — only its origin moves.
    #[test]
    fn a_drag_never_resizes_the_box() {
        for rotation in [
            PageRotation::None,
            PageRotation::Clockwise90,
            PageRotation::Clockwise180,
            PageRotation::Clockwise270,
        ] {
            let moved = dragged_box(a_box(), turned(rotation), 37.0, -21.0);

            assert_eq!(
                (moved.width, moved.height),
                (a_box().width, a_box().height),
                "{rotation:?}"
            );
        }
    }

    /// The margin clamp that used to do this went away with the margins. A
    /// box dragged hard at a corner has to stop at the page edge rather than
    /// compose a run at coordinates no page holds.
    #[test]
    fn a_drag_cannot_carry_the_box_off_the_page() {
        let page = turned(PageRotation::None);
        let (page_width, page_height) = page.unrotated();

        let off_the_bottom_left = dragged_box(a_box(), page, -9_000.0, 9_000.0);
        assert_eq!((off_the_bottom_left.x, off_the_bottom_left.y), (0.0, 0.0));

        let off_the_top_right = dragged_box(a_box(), page, 9_000.0, -9_000.0);
        assert_eq!(
            (off_the_top_right.x, off_the_top_right.y),
            (page_width - a_box().width, page_height - a_box().height)
        );
    }

    /// A box wider than the page it is on would otherwise hand `f64::clamp` a
    /// range whose low end is above its high end, which panics.
    #[test]
    fn a_box_too_big_for_its_page_clamps_rather_than_panicking() {
        let oversized = Rect {
            x: 0.0,
            y: 0.0,
            width: 10_000.0,
            height: 10_000.0,
        };

        let moved = dragged_box(oversized, turned(PageRotation::None), 50.0, 50.0);

        assert_eq!((moved.x, moved.y), (0.0, 0.0));
    }

    /// The `(x, y)` a 2D `gsk::Transform` maps `point` to.
    fn transformed(transform: &gsk::Transform, point: (f64, f64)) -> (f64, f64) {
        let (xx, yx, xy, yy, dx, dy) = transform.to_2d();
        (
            f64::from(xx) * point.0 + f64::from(xy) * point.1 + f64::from(dx),
            f64::from(yx) * point.0 + f64::from(yy) * point.1 + f64::from(dy),
        )
    }

    /// The frame and the transform inside it have to describe the same box, or
    /// the entry is turned but sitting somewhere the run is not.
    ///
    /// Checked by mapping the entry's own top-left and bottom-right through the
    /// child transform and asserting they land on opposite corners of the
    /// frame's footprint — which makes this a test of the *pairing* rather than
    /// of either half alone.
    #[gtk::test]
    fn gtk_ui_the_turned_entry_covers_exactly_the_frame_it_sits_in() {
        for rotation in [
            PageRotation::None,
            PageRotation::Clockwise90,
            PageRotation::Clockwise180,
            PageRotation::Clockwise270,
        ] {
            let page = turned(rotation);
            let bbox = a_box();
            let frame = Fixed::new();
            let entry = Entry::new();
            frame.put(&entry, 0.0, 0.0);

            place_entry(&frame, &entry, bbox, page);

            // The box `place_entry` settled on, which is the entry's own and
            // not the run's — see `entry_size`.
            let (size, _) = entry.preferred_size();
            let (width, height) = (f64::from(size.width()), f64::from(size.height()));
            let (footprint_width, footprint_height) = match rotation {
                PageRotation::Clockwise90 | PageRotation::Clockwise270 => (height, width),
                _ => (width, height),
            };
            let transform = frame.child_transform(&entry).expect("place_entry sets one");
            let corners = [
                transformed(&transform, (0.0, 0.0)),
                transformed(&transform, (width, height)),
            ];

            for (x, y) in corners {
                assert!(
                    x.abs() < 1e-3 || (x - footprint_width).abs() < 1e-3,
                    "{rotation:?}: x {x} is on neither edge of a {footprint_width} wide frame"
                );
                assert!(
                    y.abs() < 1e-3 || (y - footprint_height).abs() < 1e-3,
                    "{rotation:?}: y {y} is on neither edge of a {footprint_height} tall frame"
                );
            }
            assert!(
                (corners[0].0 - corners[1].0).abs() > 1e-3
                    && (corners[0].1 - corners[1].1).abs() > 1e-3,
                "{rotation:?}: the two corners must be opposite ones, not the same"
            );

            frame.remove(&entry);
        }
    }

    /// The canvas must stay where the reader put it.
    ///
    /// `grab_focus` on a child the `ScrolledWindow` has only just been handed
    /// is a child it has not measured, and it scrolls to the origin to reveal
    /// a widget it believes is sitting there — throwing the reader back to the
    /// start of the page with the run they were editing off screen. Only the
    /// first focus does it, which is why it cannot be caught by focusing an
    /// editor that is already open.
    ///
    /// Invisible until a page is wide enough to scroll sideways, so turning a
    /// page to landscape is what surfaced it. The fault itself is older than
    /// any of the rotation work.
    ///
    /// Built on the shell's own `Viewer` rather than on a scroller assembled
    /// here: a hand-rolled one does not reproduce it, and a test that cannot
    /// fail is worse than no test. What it cannot use is a real document —
    /// the GTK gate runs without pdfium — so the canvas is given a page-shaped
    /// widget instead, which is all the scroll position depends on.
    #[gtk::test]
    fn gtk_ui_focusing_a_fresh_editor_leaves_the_canvas_where_it_was() {
        const PARKED_AT: f64 = 300.0;

        let built = crate::app::ui_tests::built_ui();
        built.viewer.view_stack.set_visible_child_name("editor");
        built.window.set_default_size(700, 600);
        built.window.present();

        // Wider than the viewport will be, so there is a horizontal scroll
        // position to lose in the first place — which on a real document is
        // what a page turned to landscape produces.
        let page = Picture::new();
        page.set_width_request(2_000);
        page.set_height_request(1_500);
        let overlay = Overlay::new();
        overlay.set_child(Some(&page));
        overlay.set_halign(gtk::Align::Center);
        built.viewer.pages.append(&overlay);
        settle(&built.viewer.pages);

        let horizontal = built.viewer.scroll.hadjustment();
        horizontal.set_value(PARKED_AT);
        pump();
        assert_eq!(
            horizontal.value(),
            PARKED_AT,
            "the canvas has to start somewhere other than the origin, \
             or this test cannot tell a jump from a no-op"
        );

        // An editor box, added and focused the way `open_editor` does it, over
        // a part of the page the reader can see: there is nothing here for a
        // scroll to reveal.
        let entry = Entry::new();
        let frame = Fixed::new();
        frame.put(&entry, 0.0, 0.0);
        frame.set_halign(gtk::Align::Start);
        frame.set_valign(gtk::Align::Start);
        frame.set_margin_start(400);
        frame.set_margin_top(200);
        overlay.add_overlay(&frame);
        // No main-loop turn between adding the box and focusing it — that is
        // how `open_editor` does it, and an unallocated child is exactly what
        // the scrolled window mishandles.
        focus_without_scrolling(&built.viewer.scroll, &entry);
        pump();

        // Asked of the root rather than of the entry: a `GtkEntry` hands the
        // cursor to the `GtkText` inside it, so the focus widget is a
        // descendant of the box and never the box itself. Asserted at all
        // because a `grab_focus` that quietly did nothing would leave the
        // scroll untouched too, and pass the assertion below for the wrong
        // reason.
        let focused =
            gtk::prelude::RootExt::focus(&built.window).expect("something took the cursor");
        assert!(
            focused.is_ancestor(&entry),
            "the cursor has to land inside the box"
        );
        assert_eq!(
            horizontal.value(),
            PARKED_AT,
            "opening a box must not scroll the canvas"
        );

        built.viewer.state.borrow_mut().session = None;
        built.window.close();
    }

    /// Where the canvas sits once an editor opened on a parked canvas has
    /// been committed and the loop has run dry — over a lone run when
    /// `paragraph` is false, over a two-line paragraph block when it is true.
    ///
    /// The shell's own `viewer.scroll` and a real `open_editor`/`commit`, so
    /// the box that loses the cursor is the one the user typed in. No pdfium:
    /// the commit's preview refresh is held back by marking one in flight.
    fn canvas_after_a_commit(paragraph: bool) -> ((f64, f64), (f64, f64)) {
        const PARKED: (f64, f64) = (60.0, 40.0);

        let built = crate::app::ui_tests::built_ui();
        built.viewer.view_stack.set_visible_child_name("editor");
        // The window the bug was seen in. Narrower, and the tools panel
        // folds away — and with it the only widget GTK could move the cursor
        // on to, which is the move that scrolls the canvas.
        built.window.set_default_size(1_000, 720);
        built.window.present();
        struct Teardown<'a>(&'a crate::app::BuiltUi);
        impl Drop for Teardown<'_> {
            fn drop(&mut self) {
                self.0.viewer.state.borrow_mut().session = None;
                self.0.window.close();
            }
        }
        let _teardown = Teardown(&built);

        let base = gen_fixtures::build_multi_line_page_document(&["First line", "Second line"]);
        let runs = pdf_edit::read_page_content(&base, pdf_document::PageId(0))
            .expect("the fixture page parses")
            .text_runs;
        let mut session = crate::app::test_fixtures::backed_session(base);
        // Wider and taller than the viewport, so there is a position to lose.
        let page = Picture::new();
        page.set_width_request(2_000);
        page.set_height_request(1_500);
        let overlay = Overlay::new();
        overlay.set_child(Some(&page));
        overlay.set_halign(gtk::Align::Center);
        built.viewer.pages.append(&overlay);
        session
            .pages
            .push(crate::app::test_fixtures::page_slot(overlay));
        session.page_heights.push(1_500);
        {
            let mut state = built.viewer.state.borrow_mut();
            state.session = Some(session);
            state.content_edit_mode = true;
            state.preview_refresh_in_flight = true;
        }
        settle(&built.viewer.pages);

        let (horizontal, vertical) = (
            built.viewer.scroll.hadjustment(),
            built.viewer.scroll.vadjustment(),
        );
        horizontal.set_value(PARKED.0);
        vertical.set_value(PARKED.1);
        pump();
        assert_eq!(
            (horizontal.value(), vertical.value()),
            PARKED,
            "the canvas has to start away from the origin, \
             or a jump cannot be told from a no-op"
        );

        if paragraph {
            let stand_in = model::block_stand_in(runs[0].clone(), &runs);
            open_editor(&built.viewer, 0, stand_in, runs.clone());
        } else {
            open_editor(&built.viewer, 0, runs[0].clone(), Vec::new());
        }
        pump();

        let field = built
            .viewer
            .state
            .borrow()
            .session
            .as_ref()
            .and_then(|session| session.content_editor.as_ref())
            .map(|editor| editor.field.clone())
            .expect("the editor opened");
        // The cursor has to be in the box, or there is nothing for its
        // removal to hand on and the assertion below passes for free.
        let focused =
            gtk::prelude::RootExt::focus(&built.window).expect("something took the cursor");
        assert!(
            focused.is_ancestor(&field.focus_widget()) || focused == field.focus_widget(),
            "the cursor has to land inside the box"
        );
        match &field {
            EditorField::Line(entry) => entry.set_text("Edited line"),
            EditorField::Paragraph { view, .. } => view.buffer().set_text("Edited\nparagraph"),
        }

        commit(&built.viewer);
        // Long enough for an animated scroll to run its course: a jump is
        // eased in over several frames, not set at once.
        for _ in 0..3 {
            pump();
        }

        assert!(
            built
                .viewer
                .state
                .borrow()
                .session
                .as_ref()
                .is_some_and(|session| session.content_editor.is_none()),
            "the commit has to land and close the box: {:?}",
            built.viewer.status.text()
        );
        (PARKED, (horizontal.value(), vertical.value()))
    }

    /// Closing the box handed the cursor to GTK, which moved it on by
    /// itself — and the canvas viewport scrolls to reveal whatever gains
    /// focus inside it, so the page slid away from where the user was
    /// reading. Reported zoomed in: "the page goes to the middle".
    #[gtk::test]
    fn gtk_ui_committing_a_line_leaves_the_canvas_where_it_was() {
        let (parked, after) = canvas_after_a_commit(false);
        assert_eq!(after, parked, "committing a run must not scroll the canvas");
    }

    /// The same for the multi-line paragraph field, whose view sits in a
    /// scroller of its own.
    #[gtk::test]
    fn gtk_ui_committing_a_paragraph_leaves_the_canvas_where_it_was() {
        let (parked, after) = canvas_after_a_commit(true);
        assert_eq!(
            after, parked,
            "committing a paragraph must not scroll the canvas"
        );
    }

    /// Pumps the main loop until `widget` has been allocated.
    fn settle(widget: &impl IsA<gtk::Widget>) {
        let context = glib::MainContext::default();
        for _ in 0..2_000 {
            while context.iteration(false) {}
            if widget.as_ref().width() > 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    /// A click inside an open one-line box lands on the entry drawn in the
    /// run's own font, not on the page under it (which reads as a click away
    /// and commits the box shut); and text typed past the run widens the box
    /// instead of scrolling inside it.
    #[gtk::test]
    fn gtk_ui_a_camouflaged_line_takes_its_clicks_and_widens_as_typed() {
        let built = crate::app::ui_tests::built_ui();
        built.viewer.view_stack.set_visible_child_name("editor");
        built.window.set_default_size(1_000, 720);
        built.window.present();
        struct Teardown<'a>(&'a crate::app::BuiltUi);
        impl Drop for Teardown<'_> {
            fn drop(&mut self) {
                self.0.viewer.state.borrow_mut().session = None;
                self.0.window.close();
            }
        }
        let _teardown = Teardown(&built);

        let base = gen_fixtures::build_spaced_line_page_document(&["Cantidad"], 100);
        let runs = pdf_edit::read_page_content(&base, pdf_document::PageId(0))
            .expect("the fixture page parses")
            .text_runs;
        let mut session = crate::app::test_fixtures::backed_session(base);
        let scale = 6.0;
        let picture = Picture::new();
        picture.set_width_request((595.0 * scale) as i32);
        picture.set_height_request((842.0 * scale) as i32);
        let overlay = Overlay::new();
        overlay.set_child(Some(&picture));
        overlay.set_halign(gtk::Align::Center);
        built.viewer.pages.append(&overlay);
        let mut slot = crate::app::test_fixtures::page_slot(overlay.clone());
        slot.budget.factor = scale;
        session.pages.push(slot);
        session.page_heights.push((842.0 * scale) as i32);
        {
            let mut state = built.viewer.state.borrow_mut();
            state.session = Some(session);
            state.content_edit_mode = true;
            state.preview_refresh_in_flight = true;
        }
        settle(&built.viewer.pages);

        open_editor(&built.viewer, 0, runs[0].clone(), Vec::new());
        pump();

        let entry = match built
            .viewer
            .state
            .borrow()
            .session
            .as_ref()
            .and_then(|session| session.content_editor.as_ref())
            .map(|editor| editor.field.clone())
            .expect("the editor opened")
        {
            EditorField::Line(entry) => entry,
            EditorField::Paragraph { .. } => panic!("one run opens an entry"),
        };
        let bbox = runs[0].bbox;
        // As the layout left it: the window's fit may have rescaled the page.
        let page = built.viewer.state.borrow().session.as_ref().unwrap().pages[0].placement();
        let centre = place_point(
            (bbox.x + bbox.width / 2.0, bbox.y + bbox.height / 2.0),
            page,
        );
        let picked = overlay
            .pick(centre.0, centre.1, gtk::PickFlags::DEFAULT)
            .expect("something is under the click");
        assert!(
            picked == entry.clone().upcast::<gtk::Widget>() || picked.is_ancestor(&entry),
            "picked {} at {centre:?}; entry at {:?} {}x{} req {:?} frame {:?}",
            picked.type_().name(),
            entry.compute_point(&overlay, &graphene::Point::new(0.0, 0.0)),
            entry.width(),
            entry.height(),
            entry.size_request(),
            entry.parent().map(|frame| (
                frame.margin_start(),
                frame.margin_top(),
                frame.width(),
                frame.height()
            )),
        );

        // Typed past the run, the line widens rather than scrolling.
        let (opened, _) = entry.size_request();
        entry.set_text("Cantidad y mucho texto mas");
        let (typed, _) = entry.size_request();
        assert!(typed > opened, "{opened} -> {typed}");

        // Zoomed out with the box open, the next keystroke re-measures the
        // box at the new zoom — and the text it holds is drawn at it too.
        let font_size = || {
            pump();
            let description = entry.pango_context().font_description().unwrap();
            f64::from(description.size())
        };
        let before = font_size();
        if let Some(session) = built.viewer.state.borrow_mut().session.as_mut() {
            session.pages[0].budget.factor = page.scale / 2.0;
        }
        entry.set_text("Cantidad y mucho texto");
        let after = font_size();
        assert!((after / before - 0.5).abs() < 0.05, "{before} -> {after}");
    }

    /// Runs the loop dry, for the settling that is not about one widget's size.
    fn pump() {
        let context = glib::MainContext::default();
        for _ in 0..200 {
            while context.iteration(false) {}
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    /// A text run is a dozen pixels tall at a readable zoom and GTK will not
    /// shrink an `Entry` below the height of its own font. As a bare overlay
    /// child that overflow was harmless — the box just drew a little taller
    /// than the run. Inside a `Fixed` it is not: on a quarter turn the entry's
    /// height runs into negative frame x, which `GtkFixed` clamps away, and
    /// what the user gets is a sliver of a box with no text visible in it.
    ///
    /// So the frame has to be the entry's own size, turned — never the run's.
    #[gtk::test]
    fn gtk_ui_the_frame_holds_the_whole_entry_at_every_turn() {
        // A 210pt line at a 14pt size: far wider than it is tall, and far
        // shorter than an `Entry` can be drawn.
        let bbox = Rect {
            x: 72.0,
            y: 700.0,
            width: 210.0,
            height: 14.0,
        };

        for rotation in [
            PageRotation::None,
            PageRotation::Clockwise90,
            PageRotation::Clockwise180,
            PageRotation::Clockwise270,
        ] {
            let page = PagePlacement {
                scale: 0.72,
                ..turned(rotation)
            };
            let entry = Entry::new();
            entry.set_text("This file ships inside the app.");
            let frame = Fixed::new();
            frame.put(&entry, 0.0, 0.0);

            place_entry(&frame, &entry, bbox, page);

            let host = Overlay::new();
            host.set_child(Some(&gtk::DrawingArea::new()));
            host.add_overlay(&frame);
            let window = Window::new();
            window.set_default_size(1_200, 900);
            window.set_child(Some(&host));
            window.present();
            settle(&frame);

            let (minimum, natural) = entry.preferred_size();
            assert_eq!(
                (minimum.width(), minimum.height()),
                (natural.width(), natural.height()),
                "{rotation:?}: the entry must not be free to grow past the box \
                 the frame was measured for"
            );
            let (wanted_width, wanted_height) = match rotation {
                PageRotation::Clockwise90 | PageRotation::Clockwise270 => {
                    (minimum.height(), minimum.width())
                }
                _ => (minimum.width(), minimum.height()),
            };
            assert_eq!(
                (frame.width(), frame.height()),
                (wanted_width, wanted_height),
                "{rotation:?}: the frame is the entry's own box, turned"
            );

            window.destroy();
        }
    }

    /// Clicking another run while the open box holds text its font refused
    /// must keep that box, not stack a second editor over it.
    ///
    /// The reported crash: type a character the run's font cannot show (it
    /// was a `ç` in a bare Helvetica, before `pdf_edit` learned to re-encode
    /// one — `ő` has no code in any encoding Helvetica can take), press Enter
    /// — refused, box stays — then click the next line. `open_editor` replaced `content_editor` with the orphaned first
    /// box still wired, its focus-out closed the new one mid-focus-change,
    /// and GTK looped on criticals until the process was killed. Built over a
    /// real parsed page and a real slot rather than pdfium, which the GTK gate
    /// does not have: none of this needs a raster.
    #[gtk::test]
    fn gtk_ui_a_refused_retype_keeps_its_box_when_another_run_is_clicked() {
        use crate::app::state::SaveBacking;
        use crate::app::test_fixtures::model_session;
        use pdf_document::{Document, Orientation, Page, PageId, PageSize, Rotation};

        let built = crate::app::ui_tests::built_ui();
        built.viewer.view_stack.set_visible_child_name("editor");
        built.window.present();

        let base = gen_fixtures::build_multi_line_page_document(&["First line", "Second line"]);
        let runs = pdf_edit::read_page_content(&base, PageId(0))
            .expect("the fixture page parses")
            .text_runs;
        let mut session = model_session(Document::with_pages(vec![Page::base(
            PageId(0),
            0,
            PageSize::A4,
            Orientation::Portrait,
            Rotation::None,
        )]));
        session.save_backing = Some(SaveBacking {
            base: pdf_manip::LopdfDocument::from_lopdf(base).into(),
            original_bytes: Vec::new().into(),
            password: None,
        });
        let overlay = Overlay::new();
        overlay.set_child(Some(&Picture::new()));
        built.viewer.pages.append(&overlay);
        session
            .pages
            .push(crate::app::test_fixtures::page_slot(overlay.clone()));
        session.page_heights.push(842);
        built.viewer.state.borrow_mut().session = Some(session);
        pump();

        let open_entry = |viewer: &Viewer| {
            viewer
                .state
                .borrow()
                .session
                .as_ref()
                .and_then(|session| session.content_editor.as_ref())
                .map(|editor| match &editor.field {
                    EditorField::Line(entry) => (editor.run.text.clone(), entry.clone()),
                    EditorField::Paragraph { .. } => panic!("a single run opens an entry"),
                })
        };

        open_editor(&built.viewer, 0, runs[0].clone(), Vec::new());
        let (_, entry) = open_entry(&built.viewer).expect("the first click opens a box");
        entry.set_text("Kőszeg");
        entry.emit_activate();
        // Read before the loop runs: with no pdfium in the gate, a queued
        // render reports its own failure on the same status line.
        assert!(
            built.viewer.status.text().contains("cannot represent"),
            "the fixture's font has to refuse the ő, or this test proves nothing: {:?}",
            built.viewer.status.text()
        );

        open_editor(&built.viewer, 0, runs[1].clone(), Vec::new());
        pump();

        let (run_text, still_open) =
            open_entry(&built.viewer).expect("the refused box must stay open");
        assert_eq!(run_text, runs[0].text, "a second editor was opened over it");
        assert_eq!(still_open, entry);
        assert_eq!(entry.text(), "Kőszeg", "the user's text must survive");
        let mut frames = 0;
        let mut child = overlay.first_child();
        while let Some(widget) = child {
            frames += usize::from(widget.is::<Fixed>());
            child = widget.next_sibling();
        }
        assert_eq!(frames, 1, "exactly one editor box on the page");

        built.viewer.state.borrow_mut().session = None;
        built.window.close();
    }
}
