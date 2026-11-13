//! The window shortcuts, written down once (B27 decision 7).
//!
//! Each [`Shortcut`] pairs a `win.*` action name with its accelerator. The
//! code that registers the action (`connect_standard_shortcuts`,
//! `annotations::connect_history_shortcuts`) and the command palette's
//! catalog and Home's shortcut card all read these constants, so the key the
//! palette and the card show is the key GTK listens for. [`accel_label`] is
//! the one place an accelerator becomes the text the user reads.
//!
//! The text-editing keys ([`TEXT_EDITING`]) are handed to the palette's
//! search and the Find entry while they have the cursor: see
//! [`connect_search_field_handoff`].

use gtk::prelude::*;

/// One window action and the accelerator that triggers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shortcut {
    /// The action's name inside the window's `win` group, e.g. `"open"`.
    pub(crate) action: &'static str,
    /// The accelerator in GTK's syntax, e.g. `"<Control>o"`.
    pub(crate) accel: &'static str,
}

pub(crate) const OPEN: Shortcut = Shortcut {
    action: "open",
    accel: "<Control>o",
};
pub(crate) const SAVE: Shortcut = Shortcut {
    action: "save",
    accel: "<Control>s",
};
pub(crate) const PRINT: Shortcut = Shortcut {
    action: "print",
    accel: "<Control>p",
};
pub(crate) const FIND: Shortcut = Shortcut {
    action: "find",
    accel: "<Control>f",
};
pub(crate) const NEW: Shortcut = Shortcut {
    action: "new",
    accel: "<Control>n",
};
pub(crate) const UNDO: Shortcut = Shortcut {
    action: "undo",
    accel: "<Control>z",
};
pub(crate) const REDO: Shortcut = Shortcut {
    action: "redo",
    accel: "<Control>y",
};
/// Cuts the content selected in content-edit mode (B25 T-209).
pub(crate) const CUT: Shortcut = Shortcut {
    action: "cut-content",
    accel: "<Control>x",
};
/// Copies the content selection, or the selected page text.
pub(crate) const COPY: Shortcut = Shortcut {
    action: "copy",
    accel: "<Control>c",
};
/// Pastes the held content clip, or a bitmap as a stamp.
pub(crate) const PASTE: Shortcut = Shortcut {
    action: "paste",
    accel: "<Control>v",
};
/// Opens the command palette (B27).
pub(crate) const COMMAND_PALETTE: Shortcut = Shortcut {
    action: "command-palette",
    accel: "<Control>k",
};

impl Shortcut {
    /// The action's detailed name, `win.<action>`.
    pub(crate) fn detailed_action(self) -> String {
        format!("win.{}", self.action)
    }

    /// Binds the accelerator to the window action on `application`.
    pub(crate) fn install(self, application: &gtk::Application) {
        application.set_accels_for_action(&self.detailed_action(), &[self.accel]);
    }

    /// Removes the accelerator, leaving the action itself as it is.
    fn uninstall(self, application: &gtk::Application) {
        application.set_accels_for_action(&self.detailed_action(), &[]);
    }
}

/// An accelerator in GTK syntax as the user reads it, e.g. `"Ctrl+O"`, or
/// `None` when GTK cannot parse it.
///
/// The one formatter for every place a shortcut is shown — the command
/// palette's rows and Home's shortcut card — so the two cannot spell the
/// same key differently. GTK's own label rather than a hand-written
/// "Ctrl+" prefix: it is what GTK's shortcut windows and menus print, and
/// GTK translates the modifier names through its own catalog, so the text
/// follows the keyboard labels of the system locale. In the C and English
/// locales it reads "Ctrl+O", the text Home's card always showed.
pub(crate) fn accel_label(accel: &str) -> Option<String> {
    let (key, modifiers) = gtk::accelerator_parse(accel)?;
    Some(gtk::accelerator_get_label(key, modifiers).into())
}

/// The window shortcuts a text field has keys of its own for: cut, copy,
/// paste, undo and redo. While a search field has the cursor these belong
/// to the query being typed, not to the document.
pub(crate) const TEXT_EDITING: [Shortcut; 5] = [CUT, COPY, PASTE, UNDO, REDO];

/// Hands the [`TEXT_EDITING`] keys to the two search fields — the command
/// palette's and the Find entry — while one of them has the cursor, and
/// gives them back to the window when it leaves.
///
/// `win.*` accelerators are resolved before the focused widget sees the key,
/// so without this Ctrl+X in the palette's search cut the selected page
/// content and Ctrl+Z undid a document edit.
///
/// Only these two fields, named explicitly, and not every text input
/// (`annotations::text_input_has_focus`): a query is never page content,
/// but other fields keep the cursor while the user goes on working on the
/// page with the mouse — a click on the page does not move focus — so a
/// form's font-size spin button left focused would keep Ctrl+Z dead. The
/// content-edit line and paragraph editors keep document undo and the
/// content clipboard on purpose, as before.
///
/// The accelerators are unbound rather than the actions disabled, which is
/// how Delete and F2 stand down (`annotations::connect_delete_shortcut`):
/// `win.undo`/`win.redo` also drive the toolbar's and Organize's Undo/Redo
/// buttons and the palette's Undo entry, all of which read the action's
/// enabled state and must keep following the document while the cursor
/// sits in a search field. With no accelerator bound, GTK has nothing to
/// dispatch and the key goes on to the field; the actions' enabled state is
/// never touched, so nothing has to recompute it on the way back. The
/// window's `notify::focus-widget` watch decides it, as for Delete and F2.
pub(crate) fn connect_search_field_handoff(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    search_fields: [gtk::Widget; 2],
) {
    sync_search_field_handoff(application, window, &search_fields);
    window.connect_focus_widget_notify({
        let application = application.clone();
        move |window| sync_search_field_handoff(&application, window, &search_fields)
    });
}

/// Whether the window's focus is in one of `fields`. A `GtkEntry` hands its
/// focus to an inner `GtkText`, so the focus widget is a descendant.
fn focus_is_in(window: &gtk::ApplicationWindow, fields: &[gtk::Widget]) -> bool {
    // `focus` is ambiguous: `RootExt` has one too.
    gtk::prelude::GtkWindowExt::focus(window).is_some_and(|focused| {
        fields
            .iter()
            .any(|field| focused == *field || focused.is_ancestor(field))
    })
}

fn sync_search_field_handoff(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    search_fields: &[gtk::Widget],
) {
    let in_search = focus_is_in(window, search_fields);
    // Only on a change: rebinding on every focus move would make GTK rebuild
    // the window's shortcut list for nothing.
    let bound = !application
        .accels_for_action(&UNDO.detailed_action())
        .is_empty();
    if bound != in_search {
        return;
    }
    for shortcut in TEXT_EDITING {
        if in_search {
            shortcut.uninstall(application);
        } else {
            shortcut.install(application);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Shortcut; 11] = [
        OPEN,
        SAVE,
        PRINT,
        FIND,
        NEW,
        UNDO,
        REDO,
        CUT,
        COPY,
        PASTE,
        COMMAND_PALETTE,
    ];

    #[test]
    fn every_action_and_accelerator_is_used_once() {
        for (index, shortcut) in ALL.iter().enumerate() {
            for other in &ALL[index + 1..] {
                assert_ne!(shortcut.action, other.action);
                assert_ne!(shortcut.accel, other.accel);
            }
        }
    }

    #[gtk::test]
    fn gtk_ui_accel_label_is_what_the_user_reads() {
        assert_eq!(accel_label(OPEN.accel).as_deref(), Some("Ctrl+O"));
        assert_eq!(
            accel_label(COMMAND_PALETTE.accel).as_deref(),
            Some("Ctrl+K")
        );
        assert_eq!(accel_label("not an accelerator"), None);
    }

    #[test]
    fn the_detailed_name_is_in_the_window_group() {
        assert_eq!(OPEN.detailed_action(), "win.open");
    }

    mod search_field_handoff {
        use gtk::prelude::*;
        use pdf_render::PageRotation;

        use crate::app::annotations::text_input_has_focus;
        use crate::app::content_edit::clipboard;
        use crate::app::content_edit::test_support::Fixture;
        use crate::app::state::ContentSelection;
        use crate::app::update_content_edit_controls;

        const CUT: &str = "win.cut-content";
        const COPY: &str = "win.copy";
        const PASTE: &str = "win.paste";
        const UNDO: &str = "win.undo";
        const REDO: &str = "win.redo";
        const TEXT_EDITING: [&str; 5] = [CUT, COPY, PASTE, UNDO, REDO];

        fn pump() {
            let context = gtk::glib::MainContext::default();
            for _ in 0..50 {
                while context.iteration(false) {}
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }

        /// Whether a key press would reach `action`: GTK only dispatches a
        /// shortcut that one of the window's capture-phase shortcut
        /// controllers lists (that is where application accelerators land —
        /// see `palette::tests`' Ctrl+K measurement), to an action that is
        /// enabled. Either one missing and the key goes on to the focused
        /// widget.
        fn fires(fixture: &Fixture, detailed: &str) -> bool {
            let name = detailed.trim_start_matches("win.");
            let enabled = fixture
                .built
                .window
                .lookup_action(name)
                .is_some_and(|action| action.is_enabled());
            bound(fixture, detailed) && enabled
        }

        /// Whether the window's shortcut controllers carry `detailed`.
        fn bound(fixture: &Fixture, detailed: &str) -> bool {
            let controllers = fixture.built.window.observe_controllers();
            (0..controllers.n_items())
                .filter_map(|index| {
                    controllers
                        .item(index)
                        .and_downcast::<gtk::ShortcutController>()
                })
                .any(|controller| {
                    (0..controller.n_items())
                        .filter_map(|item| controller.item(item).and_downcast::<gtk::Shortcut>())
                        .any(|shortcut| {
                            shortcut
                                .action()
                                .and_downcast::<gtk::NamedAction>()
                                .is_some_and(|action| action.action_name() == detailed)
                        })
                })
        }

        /// Both lines held as a content selection, the way a marquee leaves
        /// them, with the window's controls refreshed from it.
        fn select_content(fixture: &Fixture) {
            {
                let mut state = fixture.viewer().state.borrow_mut();
                let session = state.session.as_mut().expect("a session");
                session.content_selection = Some(ContentSelection {
                    page_index: 0,
                    members: pdf_edit::Selection {
                        runs: fixture.runs.clone(),
                        ..pdf_edit::Selection::default()
                    },
                });
            }
            update_content_edit_controls(fixture.viewer());
        }

        fn selection_count(fixture: &Fixture) -> usize {
            fixture
                .viewer()
                .state
                .borrow()
                .session
                .as_ref()
                .and_then(|session| session.content_selection.as_ref())
                .map_or(0, |held| held.members.runs.len())
        }

        fn queued_edits(fixture: &Fixture) -> usize {
            fixture
                .viewer()
                .state
                .borrow()
                .session
                .as_ref()
                .and_then(|session| session.document_model.as_ref())
                .map_or(0, |model| model.pending_edits.entries().len())
        }

        /// Typing a query is not editing the page: with content selected,
        /// Ctrl+X in the palette's search field cuts the query, so the
        /// window's cut must not fire — and nothing is cut.
        #[gtk::test]
        fn gtk_ui_ctrl_x_in_the_palette_search_does_not_cut_content() {
            let fixture = Fixture::new(PageRotation::None);
            select_content(&fixture);
            assert!(fires(&fixture, CUT), "Ctrl+X cuts the selection");

            fixture.built.palette.open();
            pump();

            assert!(text_input_has_focus(fixture.viewer()));
            assert!(!fires(&fixture, CUT));
            assert_eq!(selection_count(&fixture), 2, "the content is still there");
            assert_eq!(queued_edits(&fixture), 0, "nothing was cut");
        }

        /// A held clip is pasted by Ctrl+V on the page, not from inside the
        /// palette's search field, where it pastes text.
        #[gtk::test]
        fn gtk_ui_ctrl_v_in_the_palette_search_does_not_paste_the_clip() {
            let fixture = Fixture::new(PageRotation::None);
            select_content(&fixture);
            assert!(clipboard::try_copy(fixture.viewer()), "the clip is held");
            assert!(fires(&fixture, PASTE));

            fixture.built.palette.open();
            pump();

            assert!(!fires(&fixture, PASTE));
            assert!(!fires(&fixture, COPY));
            assert_eq!(queued_edits(&fixture), 0, "nothing was pasted");
        }

        /// The Find field had the same gap before the palette existed.
        #[gtk::test]
        fn gtk_ui_ctrl_x_and_ctrl_v_stand_down_in_the_find_field() {
            let fixture = Fixture::new(PageRotation::None);
            select_content(&fixture);
            assert!(clipboard::try_copy(fixture.viewer()));
            assert!(fires(&fixture, CUT) && fires(&fixture, PASTE));

            let window = &fixture.built.window;
            gtk::prelude::GtkWindowExt::set_focus(window, Some(&fixture.viewer().search_entry));
            pump();

            assert!(text_input_has_focus(fixture.viewer()));
            for detailed in TEXT_EDITING {
                assert!(!fires(&fixture, detailed), "{detailed}");
            }
            assert_eq!(selection_count(&fixture), 2);
            assert_eq!(queued_edits(&fixture), 0);
        }

        /// Only the palette's search and the Find entry take the keys. Any
        /// other text field — a form's font-size spin button, a plain entry,
        /// the content editors — keeps them on the document: the cursor
        /// stays in such a field while the user goes on working on the page
        /// with the mouse, and Ctrl+Z must still undo there.
        #[gtk::test]
        fn gtk_ui_other_text_fields_leave_the_shortcuts_bound() {
            let fixture = Fixture::new(PageRotation::None);
            select_content(&fixture);
            assert!(clipboard::try_copy(fixture.viewer()));
            let spin = gtk::SpinButton::with_range(4.0, 400.0, 1.0);
            let entry = gtk::Entry::new();
            fixture.viewer().pages.append(&spin);
            fixture.viewer().pages.append(&entry);
            pump();

            for field in [spin.upcast_ref::<gtk::Widget>(), entry.upcast_ref()] {
                assert!(field.grab_focus(), "the test window takes focus");
                pump();
                assert!(text_input_has_focus(fixture.viewer()));
                for detailed in TEXT_EDITING {
                    assert!(bound(&fixture, detailed), "{detailed}");
                }
                assert!(fires(&fixture, CUT) && fires(&fixture, PASTE));
            }
        }

        /// Leaving the text field gives the keys back, to actions whose
        /// enabled state still follows the document, not a blanket "on".
        #[gtk::test]
        fn gtk_ui_closing_the_palette_gives_ctrl_x_back() {
            let fixture = Fixture::new(PageRotation::None);
            select_content(&fixture);
            fixture.built.palette.open();
            pump();
            assert!(!fires(&fixture, CUT));

            fixture.built.palette.close();
            pump();

            assert!(!text_input_has_focus(fixture.viewer()));
            assert!(fires(&fixture, CUT), "Ctrl+X cuts the selection again");
            assert!(fires(&fixture, COPY) && fires(&fixture, PASTE));
            assert!(bound(&fixture, UNDO) && bound(&fixture, REDO));
            // Nothing to undo: the action is still off, as the document says.
            assert!(!fires(&fixture, UNDO));
        }

        /// Ctrl+Z / Ctrl+Y in the search field never reach the document's
        /// history — and the actions themselves stay as the document has
        /// them, so the toolbar's Undo and the palette's Undo entry, which
        /// read `is_enabled()`, are not greyed out by the cursor's position.
        #[gtk::test]
        fn gtk_ui_undo_and_redo_stand_down_in_the_palette_search() {
            let fixture = Fixture::new(PageRotation::None);
            let viewer = fixture.viewer();
            select_content(&fixture);
            assert!(clipboard::try_cut(viewer), "a cut to undo");
            assert_eq!(queued_edits(&fixture), 1);
            assert!(fires(&fixture, UNDO));

            fixture.built.palette.open();
            pump();

            assert!(!fires(&fixture, UNDO));
            assert!(!fires(&fixture, REDO));
            assert!(viewer.undo_action.is_enabled(), "the document can undo");
            assert_eq!(queued_edits(&fixture), 1, "the cut is still there");
        }
    }

    #[gtk::test]
    fn gtk_ui_every_accelerator_parses() {
        for shortcut in ALL {
            assert!(
                gtk::accelerator_parse(shortcut.accel).is_some(),
                "{shortcut:?}"
            );
        }
    }
}
