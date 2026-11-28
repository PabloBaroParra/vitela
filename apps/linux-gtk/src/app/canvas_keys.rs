//! Moving through the document with the keyboard: Page Up and Page Down, Home
//! and End, the arrows.
//!
//! ## Why these are not left to the scroller
//!
//! `GtkScrolledWindow` has bindings for most of these keys, and they never
//! ran. A key press is delivered to the widget that has the focus and to its
//! ancestors; nothing on the canvas takes the focus — the pages are pictures,
//! and a click on one moves no focus — so the focus was always somewhere in
//! the toolbar or a side panel, of which the canvas's scroller is not an
//! ancestor. Its bindings were never on the key's path, and Page Down and
//! End did nothing at all.
//!
//! So the keys are read on the window, in its capture phase, and applied to
//! the canvas's adjustments directly. That makes them work wherever the
//! focus happens to rest — and makes *who the key belongs to* a decision
//! this module has to take, since it now sees the key first ([`takes`]):
//!
//! * A text field, the inline editor, a spin button, the command palette, an
//!   open popover and every control that moves something of its own with
//!   these keys (a list, a slider, a drop-down) keep all of them.
//! * Any other control — a button, a toggle, a toolbar group — has no use
//!   for Page Up, Page Down, Home or End: with the focus on one, those
//!   scroll the document. It does use the arrows — they move the focus to
//!   the next control — so the arrows stay with it.
//! * With the focus on the canvas, or nowhere, every key scrolls.
//!
//! The paging keys are given up by default and kept by a named few, rather
//! than the other way round: where the focus rests after a click on the
//! toolbar is an accident of what was clicked, and the document not
//! scrolling "because a toolbar group has the focus" is the defect this
//! module exists to end. The arrows are the opposite — kept unless the
//! canvas plainly has the focus — because moving the focus with them is
//! something every control does.
//!
//! A click on the page gives the canvas the focus, and so does a document
//! arriving on screen — an open, a switch to its tab — unless a text field
//! or a popover has it: a field keeps the cursor while the user works on
//! the page with the mouse, as it always has. So the arrows scroll a
//! document that has just been opened, and one that has just been clicked.
//! They stop when the user moves the focus to a control, and that is the
//! control's to keep: an edit made from a panel button redraws the page
//! without taking the focus off the button.
//!
//! These are not window actions: an accelerator is taken before the focused
//! widget is asked, so each key would need its action switched on and off
//! at every focus change. A decision made at the key press reads the focus
//! as it is. The keys are listed in `shortcuts::table` all the same, where
//! the conflict check and the shortcuts window read them.

use gtk::prelude::*;
use gtk::{gdk, glib, EventControllerKey, GestureClick, PropagationPhase};

use super::home::EDITOR_PAGE;
use super::layout::page_top;
use super::state::Viewer;

/// Where a key takes the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Nav {
    /// Back by most of a viewport.
    PageUp,
    /// On by most of a viewport.
    PageDown,
    /// The top of the first page.
    FirstPage,
    /// The top of the last page — and, from there on, the end of it.
    LastPage,
    LineUp,
    LineDown,
    Left,
    Right,
}

impl Nav {
    /// Whether this is an arrow key's move — the ones a focused button
    /// keeps for moving the focus.
    fn is_arrow(self) -> bool {
        matches!(self, Nav::LineUp | Nav::LineDown | Nav::Left | Nav::Right)
    }
}

/// One key and where it goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub(crate) key: gdk::Key,
    /// Whether Ctrl is held. No other modifier may be.
    pub(crate) control: bool,
    pub(crate) nav: Nav,
    /// Whether the shortcuts window prints this key. The keypad's twins of
    /// the keys above them are bound and not listed.
    pub(crate) listed: bool,
}

const fn key(key: gdk::Key, nav: Nav) -> Binding {
    Binding {
        key,
        control: false,
        nav,
        listed: true,
    }
}

const fn keypad(key: gdk::Key, nav: Nav) -> Binding {
    Binding {
        key,
        control: false,
        nav,
        listed: false,
    }
}

const fn with_control(binding: Binding) -> Binding {
    Binding {
        control: true,
        ..binding
    }
}

/// Every key this module answers.
///
/// Ctrl+Home and Ctrl+End go where Home and End go: in a document, unlike
/// in a line of text, there is no nearer "start" for the bare key to mean.
pub(crate) const BINDINGS: [Binding; 20] = [
    key(gdk::Key::Page_Up, Nav::PageUp),
    key(gdk::Key::Page_Down, Nav::PageDown),
    key(gdk::Key::Home, Nav::FirstPage),
    key(gdk::Key::End, Nav::LastPage),
    with_control(key(gdk::Key::Home, Nav::FirstPage)),
    with_control(key(gdk::Key::End, Nav::LastPage)),
    key(gdk::Key::Up, Nav::LineUp),
    key(gdk::Key::Down, Nav::LineDown),
    key(gdk::Key::Left, Nav::Left),
    key(gdk::Key::Right, Nav::Right),
    keypad(gdk::Key::KP_Page_Up, Nav::PageUp),
    keypad(gdk::Key::KP_Page_Down, Nav::PageDown),
    keypad(gdk::Key::KP_Home, Nav::FirstPage),
    keypad(gdk::Key::KP_End, Nav::LastPage),
    with_control(keypad(gdk::Key::KP_Home, Nav::FirstPage)),
    with_control(keypad(gdk::Key::KP_End, Nav::LastPage)),
    keypad(gdk::Key::KP_Up, Nav::LineUp),
    keypad(gdk::Key::KP_Down, Nav::LineDown),
    keypad(gdk::Key::KP_Left, Nav::Left),
    keypad(gdk::Key::KP_Right, Nav::Right),
];

/// The modifiers that make a key press a different key press. Caps Lock and
/// Num Lock are not among them.
fn held(modifiers: gdk::ModifierType) -> gdk::ModifierType {
    modifiers
        & (gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::SHIFT_MASK
            | gdk::ModifierType::ALT_MASK
            | gdk::ModifierType::SUPER_MASK
            | gdk::ModifierType::META_MASK
            | gdk::ModifierType::HYPER_MASK)
}

/// Where `key` with `modifiers` held goes, or `None` when it is not one of
/// this module's keys.
pub(crate) fn nav_for(key: gdk::Key, modifiers: gdk::ModifierType) -> Option<Nav> {
    let held = held(modifiers);
    let control = held == gdk::ModifierType::CONTROL_MASK;
    if !control && !held.is_empty() {
        return None;
    }
    BINDINGS
        .iter()
        .find(|binding| binding.key == key && binding.control == control)
        .map(|binding| binding.nav)
}

/// What has the keyboard focus, as far as these keys are concerned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Focus {
    /// Nothing has it.
    #[default]
    Nowhere,
    /// The canvas, or something on it that is not a text field.
    Canvas,
    /// A control with no use for the paging keys — a button, a toggle, a
    /// toolbar group, anything not named below. It keeps the arrows, which
    /// move the focus.
    Control,
    /// A control that moves something of its own with these keys: a text
    /// field, a list, a slider, a drop-down. It keeps every key.
    Keyed,
}

/// Everything the ownership rule reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Context {
    /// The editor is the screen on show, with pages to scroll.
    pub(crate) document_on_screen: bool,
    pub(crate) palette_open: bool,
    /// The focus is inside a popover.
    pub(crate) popover_focused: bool,
    pub(crate) focus: Focus,
}

/// Whether `nav`'s key is the canvas's to take in `context`. See the module
/// documentation for the rule.
pub(crate) fn takes(nav: Nav, context: Context) -> bool {
    if !context.document_on_screen || context.palette_open || context.popover_focused {
        return false;
    }
    match context.focus {
        Focus::Nowhere | Focus::Canvas => true,
        Focus::Control => !nav.is_arrow(),
        Focus::Keyed => false,
    }
}

/// The share of a viewport Page Up and Page Down move by: a few lines of
/// the previous view stay on screen to read on from.
const PAGE_SHARE: f64 = 0.9;
/// The share of a viewport one arrow press moves by.
const LINE_SHARE: f64 = 0.1;
/// The least one arrow press moves, for a viewport too small for a tenth of
/// it to be a visible step.
const LINE_MINIMUM: f64 = 24.0;

/// How far a relative move scrolls a viewport `page_size` long, signed, or
/// `None` for the moves that go to a fixed place.
pub(crate) fn step(nav: Nav, page_size: f64) -> Option<f64> {
    let page = page_size * PAGE_SHARE;
    let line = (page_size * LINE_SHARE).max(LINE_MINIMUM);
    match nav {
        Nav::PageUp => Some(-page),
        Nav::PageDown => Some(page),
        Nav::LineUp | Nav::Left => Some(-line),
        Nav::LineDown | Nav::Right => Some(line),
        Nav::FirstPage | Nav::LastPage => None,
    }
}

/// Where End takes a view at `value`: the top of the last page, so it can
/// be read from its start — or the very end of the document when the view
/// is already there or past it, which is also where a one-page document
/// goes at once. `bottom` is the furthest the view scrolls.
pub(crate) fn end_target(value: f64, last_page_top: f64, bottom: f64) -> f64 {
    let top = last_page_top.min(bottom);
    if value + 0.5 >= top {
        bottom
    } else {
        top
    }
}

fn focused(viewer: &Viewer) -> Option<gtk::Widget> {
    viewer
        .window()
        .and_then(|window| gtk::prelude::GtkWindowExt::focus(&window))
}

fn focus(viewer: &Viewer, focused: Option<&gtk::Widget>) -> Focus {
    let Some(focused) = focused else {
        return Focus::Nowhere;
    };
    if super::annotations::text_input_has_focus(viewer) || moves_with_these_keys(focused) {
        return Focus::Keyed;
    }
    let canvas: &gtk::Widget = viewer.scroll.upcast_ref();
    if focused == canvas || focused.is_ancestor(canvas) {
        return Focus::Canvas;
    }
    // The window itself holds the focus when no widget in it does.
    if focused.is::<gtk::Window>() {
        return Focus::Nowhere;
    }
    Focus::Control
}

/// Whether `widget` is a control that Page Up, Page Down, Home and End
/// already do something in: they move a slider's value, a list's cursor, a
/// drop-down's choice.
fn moves_with_these_keys(widget: &gtk::Widget) -> bool {
    widget.is::<gtk::Range>()
        || widget.is::<gtk::ListBox>()
        || widget.is::<gtk::ListBoxRow>()
        || widget.is::<gtk::ListBase>()
        || widget.is::<gtk::DropDown>()
        || widget.is::<gtk::Notebook>()
        || widget.ancestor(gtk::DropDown::static_type()).is_some()
}

/// Reads the context, or `None` while the state is mutably borrowed — a key
/// arriving in the middle of a handler is left to whoever else wants it.
fn context(viewer: &Viewer) -> Option<Context> {
    let (palette_open, has_pages) = {
        let state = viewer.state.try_borrow().ok()?;
        (
            state.palette_open,
            state
                .session
                .as_ref()
                .is_some_and(|session| !session.pages.is_empty()),
        )
    };
    let focused = focused(viewer);
    Some(Context {
        document_on_screen: has_pages
            && viewer.view_stack.visible_child_name().as_deref() == Some(EDITOR_PAGE),
        palette_open,
        popover_focused: focused
            .as_ref()
            .is_some_and(|focused| focused.ancestor(gtk::Popover::static_type()).is_some()),
        focus: focus(viewer, focused.as_ref()),
    })
}

/// Moves the view as `nav` says.
fn go(viewer: &Viewer, nav: Nav) {
    let vertical = viewer.scroll.vadjustment();
    match nav {
        Nav::FirstPage => vertical.set_value(vertical.lower()),
        Nav::LastPage => {
            let top = {
                let state = viewer.state.borrow();
                state.session.as_ref().map(|session| {
                    page_top(
                        &session.page_heights,
                        session.page_heights.len().saturating_sub(1),
                    )
                })
            };
            if let Some(top) = top {
                let bottom = (vertical.upper() - vertical.page_size()).max(vertical.lower());
                vertical.set_value(end_target(vertical.value(), top, bottom));
            }
        }
        Nav::Left | Nav::Right => {
            let horizontal = viewer.scroll.hadjustment();
            if let Some(step) = step(nav, horizontal.page_size()) {
                horizontal.set_value(horizontal.value() + step);
            }
        }
        Nav::PageUp | Nav::PageDown | Nav::LineUp | Nav::LineDown => {
            if let Some(step) = step(nav, vertical.page_size()) {
                vertical.set_value(vertical.value() + step);
            }
        }
    }
}

/// Answers one key press: scrolls and stops the key when it is the canvas's,
/// and lets it go on otherwise.
///
/// A key the canvas takes is stopped even when the view is already at the
/// end it points to, so it never falls through to move the focus instead.
pub(crate) fn handle_key(
    viewer: &Viewer,
    key: gdk::Key,
    modifiers: gdk::ModifierType,
) -> glib::Propagation {
    let Some(nav) = nav_for(key, modifiers) else {
        return glib::Propagation::Proceed;
    };
    if !context(viewer).is_some_and(|context| takes(nav, context)) {
        return glib::Propagation::Proceed;
    }
    go(viewer, nav);
    glib::Propagation::Stop
}

/// Gives the canvas the focus when a document has just arrived on screen —
/// opened in front, or its tab switched to — so the arrows scroll it
/// without a click on the page first. Like the click, it leaves a text
/// field and a popover the focus they have.
pub(crate) fn focus_on_arrival(viewer: &Viewer) {
    if context(viewer).is_some_and(|context| context.document_on_screen) {
        focus_canvas(viewer);
    }
}

/// Gives the canvas the focus for a click on the page, unless a text field
/// or a popover has it.
fn focus_canvas(viewer: &Viewer) {
    let Some(context) = context(viewer) else {
        return;
    };
    if context.popover_focused || context.palette_open || context.focus == Focus::Canvas {
        return;
    }
    if super::annotations::text_input_has_focus(viewer) {
        return;
    }
    viewer.scroll.grab_focus();
}

/// Reads the keys on `window` and makes a click on `page_area` give the
/// canvas the focus.
pub(super) fn connect(window: &gtk::ApplicationWindow, page_area: &gtk::Overlay, viewer: &Viewer) {
    // Stated rather than relied on: the canvas has to be able to hold the
    // focus for a click on the page to hand it over.
    viewer.scroll.set_focusable(true);

    let keys = EventControllerKey::new();
    // Capture, on the window: the only place every key passes whatever has
    // the focus. `handle_key` gives back the ones that are not the canvas's.
    keys.set_propagation_phase(PropagationPhase::Capture);
    keys.connect_key_pressed({
        let viewer = viewer.clone();
        move |_, key, _, modifiers| handle_key(&viewer, key, modifiers)
    });
    window.add_controller(keys);

    let click = GestureClick::new();
    // Every button, and never claimed: this only watches the press go by on
    // its way to the page's own gestures.
    click.set_button(0);
    click.set_propagation_phase(PropagationPhase::Capture);
    click.connect_pressed({
        let viewer = viewer.clone();
        move |_, _, _, _| focus_canvas(&viewer)
    });
    page_area.add_controller(click);
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: gdk::ModifierType = gdk::ModifierType::empty();
    const CONTROL: gdk::ModifierType = gdk::ModifierType::CONTROL_MASK;

    fn on_screen(focus: Focus) -> Context {
        Context {
            document_on_screen: true,
            focus,
            ..Context::default()
        }
    }

    const EVERY_NAV: [Nav; 8] = [
        Nav::PageUp,
        Nav::PageDown,
        Nav::FirstPage,
        Nav::LastPage,
        Nav::LineUp,
        Nav::LineDown,
        Nav::Left,
        Nav::Right,
    ];

    #[test]
    fn the_paging_keys_and_the_arrows_each_name_a_move() {
        for (key, nav) in [
            (gdk::Key::Page_Up, Nav::PageUp),
            (gdk::Key::Page_Down, Nav::PageDown),
            (gdk::Key::Home, Nav::FirstPage),
            (gdk::Key::End, Nav::LastPage),
            (gdk::Key::Up, Nav::LineUp),
            (gdk::Key::Down, Nav::LineDown),
            (gdk::Key::Left, Nav::Left),
            (gdk::Key::Right, Nav::Right),
            (gdk::Key::KP_Page_Down, Nav::PageDown),
            (gdk::Key::KP_End, Nav::LastPage),
        ] {
            assert_eq!(nav_for(key, NONE), Some(nav), "{key:?}");
        }
    }

    #[test]
    fn control_home_and_control_end_go_where_home_and_end_go() {
        assert_eq!(nav_for(gdk::Key::Home, CONTROL), Some(Nav::FirstPage));
        assert_eq!(nav_for(gdk::Key::End, CONTROL), Some(Nav::LastPage));
        assert_eq!(nav_for(gdk::Key::KP_Home, CONTROL), Some(Nav::FirstPage));
    }

    /// Ctrl+Page Down is the next tab's, Shift+arrows extend a selection,
    /// Alt+arrows are another program's history: none of them scrolls.
    #[test]
    fn a_key_held_with_another_modifier_is_not_a_move() {
        assert_eq!(nav_for(gdk::Key::Page_Down, CONTROL), None);
        assert_eq!(nav_for(gdk::Key::Up, CONTROL), None);
        for modifiers in [
            gdk::ModifierType::SHIFT_MASK,
            gdk::ModifierType::ALT_MASK,
            gdk::ModifierType::SUPER_MASK,
            CONTROL | gdk::ModifierType::SHIFT_MASK,
        ] {
            assert_eq!(nav_for(gdk::Key::Down, modifiers), None, "{modifiers:?}");
            assert_eq!(nav_for(gdk::Key::Home, modifiers), None, "{modifiers:?}");
        }
    }

    #[test]
    fn caps_lock_and_num_lock_do_not_change_the_key() {
        let locks = gdk::ModifierType::LOCK_MASK;
        assert_eq!(nav_for(gdk::Key::Page_Down, locks), Some(Nav::PageDown));
        assert_eq!(nav_for(gdk::Key::End, locks | CONTROL), Some(Nav::LastPage));
    }

    #[test]
    fn other_keys_are_not_moves() {
        for key in [gdk::Key::space, gdk::Key::a, gdk::Key::Tab, gdk::Key::F3] {
            assert_eq!(nav_for(key, NONE), None, "{key:?}");
        }
    }

    #[test]
    fn no_key_is_bound_twice() {
        for (index, binding) in BINDINGS.iter().enumerate() {
            for other in &BINDINGS[index + 1..] {
                assert!(
                    (binding.key, binding.control) != (other.key, other.control),
                    "{binding:?}"
                );
            }
        }
    }

    #[test]
    fn with_the_focus_on_the_canvas_or_nowhere_every_key_scrolls() {
        for focus in [Focus::Nowhere, Focus::Canvas] {
            for nav in EVERY_NAV {
                assert!(takes(nav, on_screen(focus)), "{nav:?} {focus:?}");
            }
        }
    }

    /// A button moves the focus with the arrows and does nothing with the
    /// paging keys.
    #[test]
    fn a_focused_control_keeps_the_arrows_and_gives_up_the_paging_keys() {
        let context = on_screen(Focus::Control);
        for nav in [Nav::PageUp, Nav::PageDown, Nav::FirstPage, Nav::LastPage] {
            assert!(takes(nav, context), "{nav:?}");
        }
        for nav in [Nav::LineUp, Nav::LineDown, Nav::Left, Nav::Right] {
            assert!(!takes(nav, context), "{nav:?}");
        }
    }

    #[test]
    fn a_text_field_a_list_or_a_slider_keeps_every_key() {
        for nav in EVERY_NAV {
            assert!(!takes(nav, on_screen(Focus::Keyed)), "{nav:?}");
        }
    }

    #[test]
    fn the_palette_a_popover_and_another_screen_keep_every_key() {
        for context in [
            Context {
                palette_open: true,
                ..on_screen(Focus::Nowhere)
            },
            Context {
                popover_focused: true,
                ..on_screen(Focus::Canvas)
            },
            Context {
                document_on_screen: false,
                ..on_screen(Focus::Nowhere)
            },
        ] {
            for nav in EVERY_NAV {
                assert!(!takes(nav, context), "{nav:?} {context:?}");
            }
        }
    }

    #[test]
    fn a_page_is_most_of_a_viewport_and_a_line_a_tenth_of_one() {
        assert_eq!(step(Nav::PageDown, 800.0), Some(720.0));
        assert_eq!(step(Nav::PageUp, 800.0), Some(-720.0));
        assert_eq!(step(Nav::LineDown, 800.0), Some(80.0));
        assert_eq!(step(Nav::LineUp, 800.0), Some(-80.0));
        assert_eq!(step(Nav::Right, 800.0), Some(80.0));
        assert_eq!(step(Nav::Left, 800.0), Some(-80.0));
    }

    #[test]
    fn a_line_is_never_too_small_to_see() {
        assert_eq!(step(Nav::LineDown, 100.0), Some(LINE_MINIMUM));
    }

    #[test]
    fn end_goes_to_the_top_of_the_last_page_and_then_to_the_end_of_it() {
        // Three pages; the last one starts at 2000 and the view stops at 2600.
        assert_eq!(end_target(0.0, 2000.0, 2600.0), 2000.0);
        assert_eq!(end_target(1999.0, 2000.0, 2600.0), 2000.0);
        assert_eq!(end_target(2000.0, 2000.0, 2600.0), 2600.0);
        assert_eq!(end_target(2300.0, 2000.0, 2600.0), 2600.0);
    }

    /// One page: its top is where the view already is.
    #[test]
    fn end_on_a_one_page_document_goes_straight_to_its_end() {
        assert_eq!(end_target(0.0, 0.0, 900.0), 900.0);
    }

    /// A last page shorter than the viewport cannot be scrolled to its top.
    #[test]
    fn end_never_asks_for_more_than_the_view_can_scroll() {
        assert_eq!(end_target(0.0, 2000.0, 1700.0), 1700.0);
        assert_eq!(end_target(1700.0, 2000.0, 1700.0), 1700.0);
    }

    #[test]
    fn home_and_end_go_to_a_place_not_by_a_step() {
        assert_eq!(step(Nav::FirstPage, 800.0), None);
        assert_eq!(step(Nav::LastPage, 800.0), None);
    }

    // Over a real window from here on.

    use crate::app::layout::{set_zoom, Zoom};
    use crate::app::test_support::{pump, wait_until, FileDocument};

    /// A document tall enough to scroll, in a window that is on screen.
    fn scrollable(name: &str) -> FileDocument {
        let document = FileDocument::new(name);
        document.open_file();
        set_zoom(&document.built.viewer, Zoom::Custom(2.0));
        let vertical = document.built.viewer.scroll.vadjustment();
        assert!(
            wait_until(|| {
                vertical.page_size() > 1.0
                    && vertical.upper() - vertical.page_size() > vertical.page_size()
            }),
            "the zoomed document must be more than two windows tall"
        );
        document
    }

    fn press(document: &FileDocument, key: gdk::Key) -> glib::Propagation {
        handle_key(&document.built.viewer, key, NONE)
    }

    /// The observed defect: with the focus on a control, as after a click
    /// on the toolbar, the canvas's scroller is not on the key's path and
    /// its own bindings never saw it.
    #[gtk::test]
    fn gtk_ui_page_down_and_end_scroll_the_canvas_with_the_focus_elsewhere() {
        let document = scrollable("keys-paging");
        let viewer = &document.built.viewer;
        let vertical = viewer.scroll.vadjustment();
        let canvas: &gtk::Widget = viewer.scroll.upcast_ref();
        assert!(viewer.print_button.grab_focus(), "the button takes focus");
        pump();
        let focused = focused(viewer);
        assert!(
            !focused
                .as_ref()
                .is_some_and(|focused| focused == canvas || focused.is_ancestor(canvas)),
            "the canvas does not hold the focus, which is why its bindings never ran"
        );
        assert_ne!(
            focus(viewer, focused.as_ref()),
            Focus::Keyed,
            "after an open the focus rests on {:?}, which must not keep the paging keys",
            focused.map(|focused| focused.type_().name().to_owned())
        );
        assert_eq!(vertical.value(), 0.0);

        assert_eq!(
            press(&document, gdk::Key::Page_Down),
            glib::Propagation::Stop
        );
        let paged = vertical.value();
        assert!(
            (paged - vertical.page_size() * PAGE_SHARE).abs() < 1.0,
            "one page down, not {paged}"
        );

        assert_eq!(press(&document, gdk::Key::Page_Up), glib::Propagation::Stop);
        assert_eq!(vertical.value(), 0.0);

        let last_page_top = {
            let state = viewer.state.borrow();
            let heights = &state.session.as_ref().expect("a session").page_heights;
            page_top(heights, heights.len() - 1)
        };
        let bottom = vertical.upper() - vertical.page_size();
        let expected = end_target(0.0, last_page_top, bottom);
        assert_eq!(press(&document, gdk::Key::End), glib::Propagation::Stop);
        assert_eq!(vertical.value(), expected);
        assert!(vertical.value() > paged, "End goes past one page");
        assert_eq!(press(&document, gdk::Key::End), glib::Propagation::Stop);
        assert_eq!(vertical.value(), bottom, "and then to the very end");

        assert_eq!(press(&document, gdk::Key::Home), glib::Propagation::Stop);
        assert_eq!(vertical.value(), 0.0);
    }

    /// A key the canvas takes is stopped at the end of the document too:
    /// it must not go on to move the focus.
    #[gtk::test]
    fn gtk_ui_a_paging_key_is_taken_even_at_the_top_of_the_document() {
        let document = scrollable("keys-top");

        assert_eq!(press(&document, gdk::Key::Page_Up), glib::Propagation::Stop);
        assert_eq!(document.built.viewer.scroll.vadjustment().value(), 0.0);
    }

    /// A document that has just been opened scrolls with the arrows: the
    /// canvas has the focus without a click on the page first.
    #[gtk::test]
    fn gtk_ui_a_document_just_opened_scrolls_with_the_arrows() {
        let document = scrollable("keys-arrival");
        let viewer = &document.built.viewer;

        assert_eq!(
            focused(viewer).as_ref(),
            Some(viewer.scroll.upcast_ref::<gtk::Widget>()),
            "the open gave the canvas the focus"
        );
        assert_eq!(press(&document, gdk::Key::Down), glib::Propagation::Stop);
        assert!(viewer.scroll.vadjustment().value() > 0.0);
    }

    #[gtk::test]
    fn gtk_ui_the_arrows_scroll_once_the_canvas_has_the_focus() {
        let document = scrollable("keys-arrows");
        let viewer = &document.built.viewer;
        let vertical = viewer.scroll.vadjustment();
        assert!(viewer.print_button.grab_focus(), "the button takes focus");
        pump();
        assert_eq!(press(&document, gdk::Key::Down), glib::Propagation::Proceed);

        focus_canvas(viewer);
        pump();
        assert_eq!(
            focused(viewer).as_ref(),
            Some(viewer.scroll.upcast_ref::<gtk::Widget>()),
            "a click on the page gives the canvas the focus"
        );

        assert_eq!(press(&document, gdk::Key::Down), glib::Propagation::Stop);
        let line = step(Nav::LineDown, vertical.page_size()).expect("a step");
        assert!((vertical.value() - line).abs() < 1.0);
        assert_eq!(press(&document, gdk::Key::Up), glib::Propagation::Stop);
        assert_eq!(vertical.value(), 0.0);
    }

    /// With the focus on a button the arrows are the button's — they move
    /// the focus — while Page Down still scrolls the document.
    #[gtk::test]
    fn gtk_ui_a_focused_button_keeps_the_arrows_but_not_page_down() {
        let document = scrollable("keys-button");
        let viewer = &document.built.viewer;
        assert!(viewer.print_button.grab_focus(), "the button takes focus");
        pump();

        assert_eq!(press(&document, gdk::Key::Down), glib::Propagation::Proceed);
        assert_eq!(viewer.scroll.vadjustment().value(), 0.0);
        assert_eq!(
            press(&document, gdk::Key::Page_Down),
            glib::Propagation::Stop
        );
        assert!(viewer.scroll.vadjustment().value() > 0.0);
    }

    /// A slider moves its own value with these keys, and a list its cursor.
    #[gtk::test]
    fn gtk_ui_a_slider_and_a_list_keep_every_key() {
        let document = scrollable("keys-slider");
        let viewer = &document.built.viewer;
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 10.0, 1.0);
        let list = gtk::ListBox::new();
        list.append(&gtk::Label::new(Some("row")));
        viewer.pages.append(&scale);
        viewer.pages.append(&list);
        pump();

        for widget in [scale.upcast_ref::<gtk::Widget>(), list.upcast_ref()] {
            assert!(widget.grab_focus(), "{} takes focus", widget.type_().name());
            pump();
            for key in [gdk::Key::Page_Down, gdk::Key::End, gdk::Key::Down] {
                assert_eq!(press(&document, key), glib::Propagation::Proceed, "{key:?}");
            }
        }
        assert_eq!(viewer.scroll.vadjustment().value(), 0.0);
    }

    /// A text field keeps Home, End and the arrows for its own cursor, and
    /// a click on the page does not take the cursor out of it.
    #[gtk::test]
    fn gtk_ui_a_text_field_keeps_every_key_and_its_cursor() {
        let document = scrollable("keys-field");
        let viewer = &document.built.viewer;
        let spin = gtk::SpinButton::with_range(4.0, 400.0, 1.0);
        viewer.pages.append(&spin);
        pump();
        assert!(spin.grab_focus(), "the field takes focus");
        pump();
        let before = focused(viewer);

        for key in [
            gdk::Key::Page_Down,
            gdk::Key::End,
            gdk::Key::Home,
            gdk::Key::Down,
            gdk::Key::Left,
        ] {
            assert_eq!(press(&document, key), glib::Propagation::Proceed, "{key:?}");
        }
        assert_eq!(viewer.scroll.vadjustment().value(), 0.0);

        focus_canvas(viewer);
        pump();
        assert_eq!(focused(viewer), before, "the cursor stays in the field");
    }

    #[gtk::test]
    fn gtk_ui_the_keys_are_left_alone_while_the_palette_is_open_or_on_home() {
        let document = scrollable("keys-elsewhere");
        let viewer = &document.built.viewer;

        document.built.palette.open();
        pump();
        assert_eq!(
            press(&document, gdk::Key::Page_Down),
            glib::Propagation::Proceed
        );
        document.built.palette.close();
        pump();

        crate::app::home::show_home(viewer);
        assert_eq!(
            press(&document, gdk::Key::Page_Down),
            glib::Propagation::Proceed
        );
        assert_eq!(viewer.scroll.vadjustment().value(), 0.0);
    }

    /// The window's key controllers that run in the capture phase.
    fn capture_key_controllers(window: &gtk::ApplicationWindow) -> Vec<EventControllerKey> {
        let controllers = window.observe_controllers();
        (0..controllers.n_items())
            .filter_map(|index| {
                controllers
                    .item(index)?
                    .downcast::<EventControllerKey>()
                    .ok()
            })
            .filter(|keys| keys.propagation_phase() == PropagationPhase::Capture)
            .collect()
    }

    /// The keys reach `handle_key` only through a controller on the window
    /// that runs in the capture phase — ahead of whatever has the focus.
    /// Found on the built window and fired as GTK fires it, by its
    /// `key-pressed` signal: a controller that is missing, attached
    /// elsewhere or in another phase leaves Page Down unscrolled. (GTK 4
    /// offers no public way to synthesize a key event, so the signal is as
    /// close to a real press as the suite can get.)
    #[gtk::test]
    fn gtk_ui_the_window_reads_the_paging_keys_in_the_capture_phase() {
        let document = scrollable("keys-controller");
        let viewer = &document.built.viewer;
        assert!(viewer.print_button.grab_focus(), "the button takes focus");
        pump();
        let vertical = viewer.scroll.vadjustment();
        assert_eq!(vertical.value(), 0.0);

        let taken: Vec<bool> = capture_key_controllers(&document.built.window)
            .iter()
            .map(|keys| {
                use gtk::glib::translate::IntoGlib;
                keys.emit_by_name::<bool>(
                    "key-pressed",
                    &[&gdk::Key::Page_Down.into_glib(), &0u32, &NONE],
                )
            })
            .collect();

        assert_eq!(
            taken.iter().filter(|taken| **taken).count(),
            1,
            "one capture-phase controller on the window takes Page Down: {taken:?}"
        );
        assert!(vertical.value() > 0.0, "and the canvas scrolled");
    }

    /// A press on the page hands the canvas the focus through a click
    /// gesture on the document area: any button, in the capture phase, so
    /// the page's own gestures still get the press.
    #[gtk::test]
    fn gtk_ui_a_press_on_the_page_gives_the_canvas_the_focus() {
        let document = scrollable("keys-click");
        let viewer = &document.built.viewer;
        let page_area = viewer.scroll.parent().expect("the document area");
        let controllers = page_area.observe_controllers();
        let click = (0..controllers.n_items())
            .find_map(|index| controllers.item(index)?.downcast::<GestureClick>().ok())
            .expect("the document area must have the focus click");
        assert_eq!(click.propagation_phase(), PropagationPhase::Capture);
        assert_eq!(click.button(), 0);
        assert!(viewer.print_button.grab_focus(), "the button takes focus");
        pump();

        click.emit_by_name::<()>("pressed", &[&1i32, &10.0f64, &10.0f64]);
        pump();

        assert_eq!(
            focused(viewer).as_ref(),
            Some(viewer.scroll.upcast_ref::<gtk::Widget>()),
            "the press gave the canvas the focus"
        );
    }

    #[gtk::test]
    fn gtk_ui_a_window_with_no_document_takes_no_key() {
        let built = crate::app::ui_tests::built_ui();

        assert_eq!(
            handle_key(&built.viewer, gdk::Key::Page_Down, NONE),
            glib::Propagation::Proceed
        );
        built.window.close();
    }
}
