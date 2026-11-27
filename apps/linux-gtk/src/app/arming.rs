//! What is armed: the one thing waiting for the next press on a page, the
//! cursor that says so, and the way to let go of it.
//!
//! Four features arm something — an annotation tool, a drawn signature, a
//! form-field kind, a content insert kind — and each keeps its own state and
//! its own setter. This module does not own any of that. It answers the two
//! questions that are about all four at once: *what would the next page
//! press do* ([`armed`]), and so what should the pointer look like over a
//! page ([`sync`]); and *how is it released* ([`disarm`]), which is what
//! Escape asks (`escape`).
//!
//! ## Where the cursor goes
//!
//! On each page's highlight layer, never on an ancestor. That layer is the
//! topmost child of the page, so it is what a press lands on; GTK gives the
//! press an implicit grab there, and the cursor lookup stops at the widget
//! holding the grab. A cursor set on the scroller would vanish for the
//! length of every press — the one moment it matters.
//!
//! ## Who else sets a cursor there
//!
//! * `content_edit::hover` shows a move cursor on an item's border. It only
//!   tracks while content-edit mode is on with no insert kind armed, which is
//!   exactly when nothing here is armed, and it falls back to [`cursor`]
//!   whenever it has none of its own.
//! * `pan` shows the open hand on the scroller and the closed hand on the
//!   grabbed widget. A page's own cursor would hide the open hand, so while
//!   the hand is shown ([`set_hand_shown`]) the pages show none.

use gtk::prelude::*;
use gtk::{gdk, glib};

use super::state::{ContentInsertKind, FieldKind, Tool, Viewer, ViewerState};

/// What the next press on a page would do, when it would not do the
/// ordinary thing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Armed {
    /// Place new content in content-edit mode.
    Insert(ContentInsertKind),
    /// Place a new form field in forms-edit mode.
    FormField(FieldKind),
    /// Draw or place an annotation.
    Tool(Tool),
    /// Place the drawn signature as a stamp.
    Signature,
}

/// The four armable things, as the state holds them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Modes {
    pub(crate) insert: Option<ContentInsertKind>,
    pub(crate) form_field: Option<FieldKind>,
    pub(crate) tool: Option<Tool>,
    pub(crate) signature: bool,
}

/// The cursor for a press that draws a shape or drops something at a point.
pub(crate) const PLACE: &str = "crosshair";
/// The cursor for a press that starts text.
pub(crate) const TYPE: &str = "text";
/// Every cursor this module puts on a page. [`sync`] takes back only these,
/// so a cursor someone else put there is left alone.
const OWN_CURSORS: [&str; 2] = [PLACE, TYPE];

/// Which of `modes` claims the next page press.
///
/// The setters keep them mutually exclusive, so at most one is ever set in
/// practice; the order only makes the answer a fixed one if that ever
/// slips — content-edit, then forms, then annotations, the order the page's
/// own gesture asks its modes in.
pub(crate) fn resolve(modes: Modes) -> Option<Armed> {
    modes
        .insert
        .map(Armed::Insert)
        .or(modes.form_field.map(Armed::FormField))
        .or(modes.tool.map(Armed::Tool))
        .or(modes.signature.then_some(Armed::Signature))
}

/// The standard cursor name for `armed`.
///
/// A press that starts text shows the text cursor; every other one draws a
/// shape or drops something at a point, and shows the crosshair. The markup
/// tools are among the second kind here: they drag out a band, they do not
/// follow the text under them.
pub(crate) fn cursor_name(armed: Armed) -> &'static str {
    match armed {
        Armed::Tool(Tool::FreeText) | Armed::Insert(ContentInsertKind::Text) => TYPE,
        Armed::Tool(
            Tool::Highlight
            | Tool::Underline
            | Tool::Strikeout
            | Tool::Ink
            | Tool::TextNote
            | Tool::Shape
            | Tool::Stamp,
        )
        | Armed::Insert(ContentInsertKind::Image)
        | Armed::FormField(_)
        | Armed::Signature => PLACE,
    }
}

/// The cursor a page shows: the armed one, unless the hand tool is showing
/// its own.
pub(crate) fn page_cursor(armed: Option<Armed>, hand_shown: bool) -> Option<&'static str> {
    armed.filter(|_| !hand_shown).map(cursor_name)
}

fn modes(state: &ViewerState) -> Modes {
    Modes {
        insert: state.content_insert_mode,
        form_field: state.form_field_kind,
        tool: state.active_tool,
        signature: state.signature.armed.is_some(),
    }
}

/// What is armed right now.
pub(crate) fn armed(state: &ViewerState) -> Option<Armed> {
    resolve(modes(state))
}

fn named(name: Option<&'static str>) -> Option<gdk::Cursor> {
    name.and_then(|name| gdk::Cursor::from_name(name, None))
}

/// The cursor a page shows right now, for a caller that is about to set a
/// page's cursor itself and has none of its own to show.
///
/// `None` while the state is mutably borrowed: the next [`sync`] puts the
/// armed cursor back, and a wrong cursor for a moment beats a panic inside a
/// GTK callback.
pub(crate) fn cursor(viewer: &Viewer) -> Option<gdk::Cursor> {
    let state = viewer.state.try_borrow().ok()?;
    named(page_cursor(armed(&state), viewer.hand_shown.get()))
}

/// Puts the armed cursor on every page, or takes it back, and tells Escape
/// whether it has something to release.
///
/// Called by every setter that arms or disarms, and from
/// `update_content_edit_controls`, which every session install and tab
/// switch reaches — pages built or brought back since the tool was armed
/// have never been told.
///
/// Everything is read from the state afresh, so calling it twice is the same
/// as calling it once; when the state is mutably borrowed it runs on the
/// next idle instead.
pub(crate) fn sync(viewer: &Viewer) {
    let Ok(state) = viewer.state.try_borrow() else {
        glib::idle_add_local_once({
            let viewer = viewer.clone();
            move || sync(&viewer)
        });
        return;
    };
    let wanted = page_cursor(armed(&state), viewer.hand_shown.get());
    let cursor = named(wanted);
    for page in state
        .session
        .as_ref()
        .map_or(&[][..], |session| session.pages.as_slice())
    {
        let shown = page.highlights.cursor().and_then(|cursor| cursor.name());
        let shown = shown.as_deref();
        // With nothing armed, only a cursor this module put there is taken
        // back: the hover's move cursor is not ours to clear.
        let ours = shown.is_some_and(|name| OWN_CURSORS.contains(&name));
        if shown != wanted && (wanted.is_some() || ours) {
            page.highlights.set_cursor(cursor.as_ref());
        }
    }
    drop(state);
    super::escape::update(viewer);
}

/// Records whether the hand tool is showing its cursor, and gives the pages'
/// own cursor way to it or back.
pub(crate) fn set_hand_shown(viewer: &Viewer, shown: bool) {
    if viewer.hand_shown.replace(shown) != shown {
        sync(viewer);
    }
}

/// Releases `armed` through its owner's own setter, so the owner's buttons,
/// state and status message all follow as if the user had toggled it off.
pub(crate) fn disarm(viewer: &Viewer, armed: Armed) {
    match armed {
        Armed::Insert(_) => super::content_edit::set_insert_mode(viewer, None),
        Armed::FormField(_) => super::forms::set_field_kind(viewer, None),
        Armed::Tool(tool) => super::annotations::release_tool(viewer, tool),
        Armed::Signature => super::signature::disarm(viewer),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_armed_resolves_to_nothing() {
        assert_eq!(resolve(Modes::default()), None);
    }

    #[test]
    fn each_armable_thing_resolves_to_itself() {
        assert_eq!(
            resolve(Modes {
                insert: Some(ContentInsertKind::Text),
                ..Modes::default()
            }),
            Some(Armed::Insert(ContentInsertKind::Text))
        );
        assert_eq!(
            resolve(Modes {
                form_field: Some(FieldKind::Checkbox),
                ..Modes::default()
            }),
            Some(Armed::FormField(FieldKind::Checkbox))
        );
        assert_eq!(
            resolve(Modes {
                tool: Some(Tool::Ink),
                ..Modes::default()
            }),
            Some(Armed::Tool(Tool::Ink))
        );
        assert_eq!(
            resolve(Modes {
                signature: true,
                ..Modes::default()
            }),
            Some(Armed::Signature)
        );
    }

    /// The setters never leave two armed, but the answer must not depend on
    /// that: it follows the order a page press is dispatched in.
    #[test]
    fn several_at_once_resolve_in_press_order() {
        let all = Modes {
            insert: Some(ContentInsertKind::Image),
            form_field: Some(FieldKind::Text),
            tool: Some(Tool::Shape),
            signature: true,
        };
        assert_eq!(resolve(all), Some(Armed::Insert(ContentInsertKind::Image)));
        assert_eq!(
            resolve(Modes {
                insert: None,
                ..all
            }),
            Some(Armed::FormField(FieldKind::Text))
        );
        assert_eq!(
            resolve(Modes {
                insert: None,
                form_field: None,
                ..all
            }),
            Some(Armed::Tool(Tool::Shape))
        );
    }

    #[test]
    fn text_tools_show_the_text_cursor_and_the_rest_a_crosshair() {
        assert_eq!(cursor_name(Armed::Tool(Tool::FreeText)), TYPE);
        assert_eq!(cursor_name(Armed::Insert(ContentInsertKind::Text)), TYPE);
        for tool in Tool::ALL {
            if tool != Tool::FreeText {
                assert_eq!(cursor_name(Armed::Tool(tool)), PLACE, "{tool:?}");
            }
        }
        assert_eq!(cursor_name(Armed::Insert(ContentInsertKind::Image)), PLACE);
        assert_eq!(cursor_name(Armed::Signature), PLACE);
        for kind in [
            FieldKind::Text,
            FieldKind::Checkbox,
            FieldKind::RadioGroup,
            FieldKind::Dropdown,
        ] {
            assert_eq!(cursor_name(Armed::FormField(kind)), PLACE, "{kind:?}");
        }
    }

    #[test]
    fn every_cursor_handed_out_is_one_sync_takes_back() {
        for tool in Tool::ALL {
            assert!(OWN_CURSORS.contains(&cursor_name(Armed::Tool(tool))));
        }
        assert!(OWN_CURSORS.contains(&cursor_name(Armed::Signature)));
    }

    #[test]
    fn a_page_shows_the_armed_cursor_unless_the_hand_is_shown() {
        let ink = Some(Armed::Tool(Tool::Ink));
        assert_eq!(page_cursor(ink, false), Some(PLACE));
        assert_eq!(page_cursor(ink, true), None, "the hand wins");
        assert_eq!(page_cursor(None, false), None);
        assert_eq!(page_cursor(None, true), None);
    }
}
