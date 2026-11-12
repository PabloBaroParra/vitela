//! The multi-line text field the inline editor (`super::editor`) opens over
//! a paragraph block — several lines of one font a leading apart, which
//! `pdf_edit::group_text_runs` reads as one block.
//!
//! A one-line `Entry` cannot hold a paragraph: its line breaks are part of
//! what the reader retypes, and the core wraps each typed line to the
//! paragraph's width (`pdf_save::retype_text_block_of`). So a paragraph gets
//! a wrapping `TextView` sized to its box, where **Enter** is a line break,
//! **Ctrl+Enter** or moving the focus away commits, and **Esc** cancels —
//! everything else about the editor (refusals, amends, the frame that turns
//! with a rotated page) is the one-line editor's, in `super::editor`.
//!
//! The view sits in a `ScrolledWindow` that pins its size: a `TextView`'s
//! natural size follows its text, and `GtkFixed` sizes a child by its
//! natural, so a bare view would grow under the user as they typed while the
//! frame's turn went on translating by the size the box opened with — the
//! trap `super::editor`'s `entry_size` describes for the entry.

use gtk::prelude::*;
use gtk::{
    gdk, glib, pango, EventControllerFocus, EventControllerKey, PolicyType, PropagationPhase,
    ScrolledWindow, TextView, WrapMode,
};
use pdf_document::Rect;
use pdf_render::PagePlacement;

use crate::app::state::{EditorField, Viewer};

/// The smallest box the view is given, in pixels, so a paragraph at a tiny
/// zoom still leaves room to see what is being typed.
const MIN_WIDTH_PX: i32 = 80;

/// What a key pressed in the paragraph view does.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum KeyAction {
    /// Ctrl+Enter: record the paragraph and close the box.
    Commit,
    /// Esc: close the box and record nothing.
    Cancel,
    /// Anything else, plain Enter among it: the view's own — a line break.
    Type,
}

/// What `keyval` with `modifiers` held does in the paragraph view.
pub(super) fn key_action(keyval: gdk::Key, modifiers: gdk::ModifierType) -> KeyAction {
    let enter = matches!(
        keyval,
        gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter
    );
    if keyval == gdk::Key::Escape {
        KeyAction::Cancel
    } else if enter && modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
        KeyAction::Commit
    } else {
        KeyAction::Type
    }
}

/// A wrapping view holding `text`, all of it selected, inside the scroller
/// that will fix its size.
pub(super) fn new_field(text: &str) -> EditorField {
    let view = TextView::new();
    // By word, falling back to characters for a word wider than the box:
    // plain `Word` lets one overlong word run off the box unseen.
    view.set_wrap_mode(WrapMode::WordChar);
    view.set_accepts_tab(false);
    let buffer = view.buffer();
    buffer.set_text(text);
    let (start, end) = buffer.bounds();
    buffer.select_range(&start, &end);

    let scroller = ScrolledWindow::new();
    scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
    scroller.set_propagate_natural_width(false);
    scroller.set_propagate_natural_height(false);
    scroller.set_child(Some(&view));
    EditorField::Paragraph { view, scroller }
}

/// The size the paragraph's box asks for at `page`'s zoom, in the box's own
/// upright frame, pinned on `scroller` as its exact size.
///
/// The width is the paragraph's, so the view wraps where the page roughly
/// will. The height is the paragraph's too, grown to fit the text as it
/// opens in the theme's font: the view does not draw the PDF's font, and a
/// box that clipped the paragraph's last lines would hide what is retyped.
///
/// The text is laid out with Pango rather than asked of the view: a
/// `TextView` validates its layout lazily, so before it is on screen its
/// measured height is a line or two, whatever it holds.
pub(super) fn field_size(
    view: &TextView,
    scroller: &ScrolledWindow,
    bbox: Rect,
    page: PagePlacement,
) -> (f64, f64) {
    let width = ((bbox.width * page.scale).round() as i32).max(MIN_WIDTH_PX);
    let buffer = view.buffer();
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
    let layout = view.create_pango_layout(Some(&text));
    layout.set_width(width * pango::SCALE);
    layout.set_wrap(pango::WrapMode::WordChar);
    let (_, text_height) = layout.pixel_size();
    let natural = text_height + view.top_margin() + view.bottom_margin();
    let height = ((bbox.height * page.scale).round() as i32)
        .max(natural)
        .max(1);
    scroller.set_size_request(width, height);
    (f64::from(width), f64::from(height))
}

/// Wires the view's keys and focus to the editor's commit and cancel.
pub(super) fn wire(viewer: &Viewer, view: &TextView) {
    let focus = EventControllerFocus::new();
    focus.connect_leave({
        let viewer = viewer.clone();
        move |_| super::editor::commit(&viewer)
    });
    view.add_controller(focus);

    let keys = EventControllerKey::new();
    // Capture phase, ahead of the view's own key handling, which would
    // otherwise take Ctrl+Enter as a line break too.
    keys.set_propagation_phase(PropagationPhase::Capture);
    keys.connect_key_pressed({
        let viewer = viewer.clone();
        move |_, keyval, _keycode, modifiers| match key_action(keyval, modifiers) {
            KeyAction::Commit => {
                super::editor::commit(&viewer);
                glib::Propagation::Stop
            }
            KeyAction::Cancel => {
                super::editor::cancel(&viewer);
                glib::Propagation::Stop
            }
            KeyAction::Type => glib::Propagation::Proceed,
        }
    });
    view.add_controller(keys);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plain Enter is the reader's line break, never a commit: the paragraph
    /// view is where a line is added.
    #[test]
    fn plain_enter_types_a_line_break() {
        let none = gdk::ModifierType::empty();
        assert_eq!(key_action(gdk::Key::Return, none), KeyAction::Type);
        assert_eq!(key_action(gdk::Key::KP_Enter, none), KeyAction::Type);
        assert_eq!(
            key_action(gdk::Key::Return, gdk::ModifierType::SHIFT_MASK),
            KeyAction::Type
        );
    }

    #[test]
    fn ctrl_enter_commits() {
        let ctrl = gdk::ModifierType::CONTROL_MASK;
        assert_eq!(key_action(gdk::Key::Return, ctrl), KeyAction::Commit);
        assert_eq!(key_action(gdk::Key::KP_Enter, ctrl), KeyAction::Commit);
    }

    #[test]
    fn escape_cancels_and_letters_type() {
        let none = gdk::ModifierType::empty();
        assert_eq!(key_action(gdk::Key::Escape, none), KeyAction::Cancel);
        assert_eq!(key_action(gdk::Key::a, none), KeyAction::Type);
    }
}

#[cfg(test)]
mod gtk_tests {
    use gtk::prelude::*;
    use gtk::{Overlay, Picture};
    use pdf_document::{Command, PageId, TextRun};

    use crate::app::i18n::tr;
    use crate::app::state::{EditorField, Viewer};
    use crate::app::ui_tests::built_ui;
    use crate::app::BuiltUi;

    use super::super::editor;

    /// A window over a real page whose two lines read as one paragraph, with
    /// the editor open over it. No pdfium: a commit's refresh is held back by
    /// marking one already in flight, so it is queued, not run.
    struct Fixture {
        built: BuiltUi,
        runs: Vec<TextRun>,
    }

    impl Fixture {
        fn open() -> Self {
            let built = built_ui();
            built.viewer.view_stack.set_visible_child_name("editor");
            built.window.present();

            let base = gen_fixtures::build_multi_line_page_document(&["First line", "Second line"]);
            let runs = pdf_edit::read_page_content(&base, PageId(0))
                .expect("the fixture page parses")
                .text_runs;
            let mut session = crate::app::test_fixtures::backed_session(base);
            let overlay = Overlay::new();
            overlay.set_child(Some(&Picture::new()));
            built.viewer.pages.append(&overlay);
            session
                .pages
                .push(crate::app::test_fixtures::page_slot(overlay));
            session.page_heights.push(842);
            built.viewer.state.borrow_mut().session = Some(session);
            built.viewer.state.borrow_mut().content_edit_mode = true;
            built.viewer.state.borrow_mut().preview_refresh_in_flight = true;

            let stand_in = super::super::model::block_stand_in(runs[0].clone(), &runs);
            editor::open_editor(&built.viewer, 0, stand_in, runs.clone());
            Self { built, runs }
        }

        fn viewer(&self) -> &Viewer {
            &self.built.viewer
        }

        fn view(&self) -> gtk::TextView {
            match self.field().expect("the editor is open") {
                EditorField::Paragraph { view, .. } => view,
                EditorField::Line(_) => panic!("a paragraph opens the multi-line field"),
            }
        }

        fn field(&self) -> Option<EditorField> {
            self.viewer()
                .state
                .borrow()
                .session
                .as_ref()
                .and_then(|session| session.content_editor.as_ref())
                .map(|editor| editor.field.clone())
        }

        fn entries(&self) -> Vec<Command> {
            self.viewer()
                .state
                .borrow()
                .session
                .as_ref()
                .and_then(|session| session.document_model.as_ref())
                .expect("a model")
                .pending_edits
                .entries()
                .to_vec()
        }

        fn type_and_commit(&self, text: &str) {
            self.view().buffer().set_text(text);
            editor::commit(self.viewer());
        }
    }

    /// Teardown on the unwind path too: `#[gtk::test]` cases share one main
    /// thread. The session goes first so the close raises no unsaved-changes
    /// prompt.
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.built.viewer.state.borrow_mut().session = None;
            self.built.window.close();
        }
    }

    /// The view is the paragraph's: its text, a line break per line, and
    /// wrapping by word (characters only for a word wider than the box).
    #[gtk::test]
    fn gtk_ui_the_paragraph_opens_in_a_wrapping_view() {
        let fixture = Fixture::open();

        let view = fixture.view();
        assert_eq!(view.wrap_mode(), gtk::WrapMode::WordChar);
        assert_eq!(
            fixture.field().expect("open").text(),
            "First line\nSecond line"
        );
        let (start, end) = view.buffer().bounds();
        assert_eq!(
            view.buffer().selection_bounds(),
            Some((start, end)),
            "all of it selected, as the entry selects its text"
        );
    }

    /// A line typed onto the paragraph is one queued batch that inserts a
    /// line styled after the paragraph's first.
    #[gtk::test]
    fn gtk_ui_a_paragraph_grown_by_a_line_records_one_batch() {
        let fixture = Fixture::open();

        fixture.type_and_commit("First line\nSecond line\nThird line");

        assert!(fixture.field().is_none(), "the editor closed");
        let entries = fixture.entries();
        let [Command::Batch(children)] = entries.as_slice() else {
            panic!("expected one batch, got {entries:?}");
        };
        assert!(
            children.iter().any(|child| matches!(
                child,
                Command::InsertTextRunLike { item, template }
                    if item.text == "Third line" && template.id == fixture.runs[0].id
            )),
            "{children:?}"
        );
        assert_eq!(
            fixture
                .viewer()
                .state
                .borrow()
                .preview_refresh_pending
                .as_deref(),
            Some(tr("edit-text-updated").as_str())
        );
    }

    /// Fewer lines than the paragraph had removes the lines it no longer
    /// needs, in the same one batch.
    #[gtk::test]
    fn gtk_ui_a_paragraph_shrunk_to_one_line_removes_the_rest() {
        let fixture = Fixture::open();

        fixture.type_and_commit("One line");

        let entries = fixture.entries();
        let [Command::Batch(children)] = entries.as_slice() else {
            panic!("expected one batch, got {entries:?}");
        };
        assert!(
            children.contains(&Command::RemoveTextRun(fixture.runs[1].clone())),
            "{children:#?}"
        );
        assert!(children.contains(&Command::ReplaceTextRunContent {
            item: fixture.runs[0].clone(),
            after: "One line".to_string(),
        }));
    }

    /// Text that is too long for one line wraps onto more lines, to the
    /// paragraph's width, without the reader typing a break.
    #[gtk::test]
    fn gtk_ui_a_long_line_wraps_to_the_paragraphs_width() {
        let fixture = Fixture::open();

        fixture.type_and_commit("First line now runs on well past the width it had\nSecond line");

        let entries = fixture.entries();
        let [Command::Batch(children)] = entries.as_slice() else {
            panic!("expected one batch, got {entries:?}");
        };
        let inserted = children
            .iter()
            .filter(|child| matches!(child, Command::InsertTextRunLike { .. }))
            .count();
        assert!(inserted >= 1, "the long line wrapped: {children:?}");
    }

    /// Esc closes the box and records nothing.
    #[gtk::test]
    fn gtk_ui_escape_cancels_a_paragraph_edit() {
        let fixture = Fixture::open();
        fixture
            .view()
            .buffer()
            .set_text("First line\nSecond line\nThird line");

        editor::cancel(fixture.viewer());

        assert!(fixture.field().is_none());
        assert!(fixture.entries().is_empty());
        assert_eq!(fixture.viewer().status.text(), tr("edit-cancelled"));
    }

    /// The view draws the theme's font, not the PDF's, so a paragraph whose
    /// box is shorter than its lines in that font opens tall enough to show
    /// every line rather than clipping the last ones. Measured before the
    /// box is on screen, as `open_editor` measures it.
    #[gtk::test]
    fn gtk_ui_the_box_opens_tall_enough_for_every_line() {
        let field = super::new_field("One\nTwo\nThree\nFour");
        let EditorField::Paragraph { view, scroller } = &field else {
            panic!("a paragraph field");
        };
        let bbox = pdf_document::Rect {
            x: 100.0,
            y: 600.0,
            width: 200.0,
            height: 10.0,
        };
        let page = pdf_render::PagePlacement {
            width_pt: 612.0,
            height_pt: 792.0,
            rotation: pdf_render::PageRotation::None,
            scale: 1.0,
        };

        let (_, height) = super::field_size(view, scroller, bbox, page);

        // A line of a laid-out paragraph rounds a pixel short of a lone
        // line, so four of them may come out a few pixels under 4 x `line`.
        let (_, line) = view.create_pango_layout(Some("X")).pixel_size();
        assert!(
            height >= f64::from(4 * line - 4),
            "{height} px for four lines of {line} px"
        );
    }

    /// The paragraph unchanged is not an edit.
    #[gtk::test]
    fn gtk_ui_committing_the_paragraph_unchanged_records_nothing() {
        let fixture = Fixture::open();

        editor::commit(fixture.viewer());

        assert!(fixture.field().is_none());
        assert!(fixture.entries().is_empty());
    }
}
