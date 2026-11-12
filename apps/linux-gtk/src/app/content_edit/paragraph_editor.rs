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

use super::camouflage::{box_geometry, EditorLook, LineMetrics};
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
    // No scrollbar to draw over the page: the box grows to fit its text
    // (`super::editor::refit`), and scrolls without one if it ever must.
    scroller.set_policy(PolicyType::Never, PolicyType::External);
    scroller.set_propagate_natural_width(false);
    scroller.set_propagate_natural_height(false);
    scroller.set_child(Some(&view));
    EditorField::Paragraph { view, scroller }
}

/// Where the paragraph's box goes and the size it asks for at `page`'s
/// zoom, in the box's own upright frame, pinned on `scroller` as its exact
/// size.
///
/// Drawn in `look` — the paragraph's own font, as the editor opens over
/// text already on the page — the box is `super::camouflage::box_geometry`'s:
/// the block's width, so the view wraps where the page will, and its first
/// baseline on the block's. Without a look it is `bbox` at the zoom, in the
/// theme's font. Either way it is grown to fit the text, so no line is ever
/// clipped out of sight.
///
/// The text is laid out with Pango rather than asked of the view: a
/// `TextView` validates its layout lazily, so before it is on screen its
/// measured height is a line or two, whatever it holds.
pub(super) fn field_size(
    view: &TextView,
    scroller: &ScrolledWindow,
    bbox: Rect,
    page: PagePlacement,
    look: Option<&EditorLook>,
) -> (Rect, (f64, f64)) {
    let buffer = view.buffer();
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
    let layout = view.create_pango_layout(Some(&text));
    layout.set_wrap(pango::WrapMode::WordChar);
    let Some(look) = look else {
        let width = ((bbox.width * page.scale).round() as i32).max(MIN_WIDTH_PX);
        layout.set_width(width * pango::SCALE);
        let (_, text_height) = layout.pixel_size();
        let natural = text_height + view.top_margin() + view.bottom_margin();
        let height = ((bbox.height * page.scale).round() as i32)
            .max(natural)
            .max(1);
        scroller.set_size_request(width, height);
        return (bbox, (f64::from(width), f64::from(height)));
    };

    let metrics = LineMetrics::of(&view.pango_context(), look, page.scale);
    let width = box_geometry(look, page.scale, metrics, 0.0).width_px;
    layout.set_font_description(Some(&look.font_description(page.scale)));
    layout.set_attributes(Some(&look.attributes(page.scale)));
    layout.set_width((width * f64::from(pango::SCALE)) as i32);
    let (_, text_height) = layout.pixel_size();
    let geometry = box_geometry(look, page.scale, metrics, f64::from(text_height));
    scroller.set_size_request(geometry.width_px as i32, geometry.height_px as i32);
    (geometry.placed, (geometry.width_px, geometry.height_px))
}

/// Wires the view's keys and focus to the editor's commit and cancel.
pub(super) fn wire(viewer: &Viewer, view: &TextView) {
    // A line typed or wrapped onto the paragraph grows the box under it.
    view.buffer().connect_changed({
        let viewer = viewer.clone();
        move |_| super::editor::refit(&viewer)
    });

    let focus = EventControllerFocus::new();
    focus.connect_leave({
        let viewer = viewer.clone();
        // Weak: the controller lives on this very widget.
        let view = view.upcast_ref::<gtk::Widget>().downgrade();
        move |_| super::editor::commit_on_focus_leave(&viewer, view.clone())
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
    use gtk::{glib, Overlay, Picture};
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
            let base = gen_fixtures::build_multi_line_page_document(&["First line", "Second line"]);
            let runs = pdf_edit::read_page_content(&base, PageId(0))
                .expect("the fixture page parses")
                .text_runs;
            Self::open_on(base, runs)
        }

        /// The editor over the committed reportlab quote's accented spec
        /// paragraph, whose accents only the font's /ToUnicode explains.
        fn open_accented_quote() -> Self {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/content-edit/reportlab_quote_paragraph.pdf");
            let base = lopdf::Document::load(path).expect("committed fixture loads");
            let page = pdf_edit::read_page_content(&base, PageId(0))
                .expect("the fixture page parses")
                .text_runs;
            let spec = pdf_edit::group_text_runs(&page)
                .into_iter()
                .find(|block| block.lines.len() > 1)
                .expect("the spec paragraph");
            let runs = spec
                .runs
                .iter()
                .map(|id| page.iter().find(|run| run.id == *id).unwrap().clone())
                .collect();
            Self::open_on(base, runs)
        }

        fn open_on(base: lopdf::Document, runs: Vec<TextRun>) -> Self {
            let built = built_ui();
            built.viewer.view_stack.set_visible_child_name("editor");
            built.window.present();

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

        fn look(&self) -> super::super::camouflage::EditorLook {
            self.viewer()
                .state
                .borrow()
                .session
                .as_ref()
                .and_then(|session| session.content_editor.as_ref())
                .and_then(|editor| editor.camouflage.as_ref())
                .map(|camouflage| camouflage.look.clone())
                .expect("a retype editor is camouflaged")
        }

        fn frame(&self) -> gtk::Fixed {
            self.viewer()
                .state
                .borrow()
                .session
                .as_ref()
                .and_then(|session| session.content_editor.as_ref())
                .map(|editor| editor.frame.clone())
                .expect("the editor is open")
        }

        fn scroller(&self) -> gtk::ScrolledWindow {
            match self.field().expect("the editor is open") {
                EditorField::Paragraph { scroller, .. } => scroller,
                EditorField::Line(_) => panic!("a paragraph opens the multi-line field"),
            }
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

    /// Runs the loop until GTK has styled and laid out what is on screen.
    fn pump() {
        let context = glib::MainContext::default();
        for _ in 0..100 {
            while context.iteration(false) {}
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    fn close(left: f64, right: f64, tolerance: f64) -> bool {
        (left - right).abs() <= tolerance
    }

    /// The quote's spec paragraph is Vera 9 on an 11 pt leading, dark blue,
    /// 0.3 pt apart, from a subset TrueType program the file embeds. The
    /// editor asks for that program's own family when this Pango can load
    /// it (1.56+), and for a family matched from its name when it cannot.
    #[gtk::test]
    fn gtk_ui_the_paragraph_is_drawn_in_the_pdfs_font_size_colour_and_leading() {
        let fixture = Fixture::open_accented_quote();
        let look = fixture.look();

        if super::super::camouflage::can_register() {
            let [family] = look.font.families.as_slice() else {
                panic!("the embedded program's family alone: {:?}", look.font);
            };
            let font_map = fixture.view().pango_context().font_map().unwrap();
            assert!(
                font_map
                    .list_families()
                    .iter()
                    .any(|known| known.name() == family.as_str()),
                "{family} is registered"
            );
        } else {
            assert_eq!(look.font.families, ["Vera", "sans-serif"]);
        }
        assert!(close(look.size_pt, 9.0, 1e-6), "{}", look.size_pt);
        assert!(close(look.leading_pt.unwrap(), 11.0, 1e-3));
        assert!(close(look.letter_spacing_pt, 0.3, 1e-6));
        let [r, g, b] = look.colour;
        assert!(close(r, 0.1, 1e-6) && close(g, 0.2, 1e-6) && close(b, 0.55, 1e-6));
        assert_eq!(fixture.view().justification(), gtk::Justification::Left);

        // And GTK draws it so: the view's own font is the look's.
        pump();
        let drawn = fixture.view().pango_context().font_description().unwrap();
        assert_eq!(
            drawn.family().map(|family| family.to_string()),
            Some(look.font.families.join(",")),
        );
        // The fixture's page is at 100%, so 9 pt is 9 px.
        let size = f64::from(drawn.size()) / f64::from(gtk::pango::SCALE);
        assert!(close(size, 9.0, 0.5), "{size}");
    }

    /// A standard-14 font embeds no program: the family comes from its name.
    #[gtk::test]
    fn gtk_ui_a_standard_font_falls_back_to_a_family_matched_by_name() {
        let fixture = Fixture::open();
        let look = fixture.look();

        assert_eq!(look.font.families, ["Helvetica", "sans-serif"]);
        assert!(close(look.size_pt, 24.0, 1e-6));
        assert_eq!(look.colour, [0.0; 3]);
        assert_eq!(look.background, [1.0; 3], "no render: white paper");
    }

    /// The box's top sits one first-baseline offset — the font's ascent,
    /// moved by half the leading's difference from the font's height —
    /// above the paragraph's first baseline, so typed text sits on it.
    #[gtk::test]
    fn gtk_ui_the_first_line_lands_on_the_paragraphs_baseline() {
        use super::super::camouflage::{first_baseline_offset, LineMetrics};
        let fixture = Fixture::open_accented_quote();
        let look = fixture.look();
        let scale = 1.0;
        let metrics = LineMetrics::of(&fixture.view().pango_context(), &look, scale);

        let top_pt = look.first_baseline
            + first_baseline_offset(metrics, look.line_height_px(scale)) / scale;

        // The test page is 842 pt tall at 100%: the frame's margin is the
        // box top's distance from the page's top.
        let expected = (842.0 - top_pt) * scale;
        let margin = f64::from(fixture.frame().margin_top());
        assert!(close(margin, expected, 1.0), "{margin} vs {expected}");
        assert_eq!(
            f64::from(fixture.frame().margin_start()),
            (look.block.x * scale).round()
        );
    }

    /// Lines typed onto the paragraph grow the box down to hold them.
    #[gtk::test]
    fn gtk_ui_the_box_grows_as_lines_are_typed() {
        let fixture = Fixture::open();
        let (_, before) = fixture.scroller().size_request();

        fixture
            .view()
            .buffer()
            .set_text("First line\nSecond line\nThird\nFourth\nFifth\nSixth");

        let (_, after) = fixture.scroller().size_request();
        assert!(after > before, "{before} -> {after}");
    }

    /// The view is the paragraph's: its text, a line break per line, and
    /// wrapping by word (characters only for a word wider than the box).
    /// An accented paragraph reads with its accents and commits: the
    /// editor closes instead of staying open on a "cannot represent" error.
    #[gtk::test]
    fn gtk_ui_an_accented_paragraph_commits_and_closes_the_editor() {
        let fixture = Fixture::open_accented_quote();
        let text = fixture.field().expect("open").text();
        assert!(text.starts_with("COMPOSICIÓN"), "{text:?}");

        fixture.type_and_commit(
            "COMPOSICIÓN: Aluminio térmico
GARANTÍA: Instalación incluida",
        );

        assert!(fixture.field().is_none(), "the editor closed");
        assert_eq!(fixture.entries().len(), 1, "one queued batch");
    }

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

        let (_, (_, height)) = super::field_size(view, scroller, bbox, page, None);

        // A line of a laid-out paragraph rounds a pixel short of a lone
        // line, so four of them may come out a few pixels under 4 x `line`.
        let (_, line) = view.create_pango_layout(Some("X")).pixel_size();
        assert!(
            height >= f64::from(4 * line - 4),
            "{height} px for four lines of {line} px"
        );
    }

    /// Clicking a button that takes the focus — the toolbar's panel toggles,
    /// say — while the box is open moves the focus out of it, and that
    /// commits the paragraph. The commit must not take the box off the page
    /// while GTK is still walking the old focus's ancestors: the walk then
    /// never meets the common ancestor and spins on
    /// `gtk_widget_get_parent (NULL)` for ever. Criticals are made fatal so
    /// that regression aborts the run instead of hanging it.
    #[gtk::test]
    fn gtk_ui_focusing_a_toolbar_button_commits_the_paragraph_without_hanging() {
        // Restored on the way out, pass or fail: every gtk_ui_ test shares
        // this one thread, and the next one's criticals are not ours.
        struct RestoreFatal(glib::LogLevels);
        impl Drop for RestoreFatal {
            fn drop(&mut self) {
                glib::log_set_always_fatal(self.0);
            }
        }
        let _restore = RestoreFatal(glib::log_set_always_fatal(glib::LogLevels::LEVEL_CRITICAL));
        let fixture = Fixture::open();
        fixture.view().buffer().set_text(
            "First line
Second line changed",
        );
        let button = fixture.viewer().print_button.clone();
        button.set_sensitive(true);

        button.grab_focus();
        let context = glib::MainContext::default();
        for _ in 0..200 {
            if fixture.field().is_none() {
                break;
            }
            while context.iteration(false) {}
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(fixture.field().is_none(), "the focus-out closed the box");
        assert_eq!(fixture.entries().len(), 1, "and recorded the paragraph");
        assert!(button.has_focus(), "the focus landed on the button");
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
