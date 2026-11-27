//! Where a split's files go, and whether they may land there.
//!
//! Everything this chain does *before* a byte is produced but *after* the
//! cuts are settled — the folder chooser and the one overwrite guard — which
//! is the half [`chooser`](super::super::chooser) owns for the chains that
//! name a single file. It is separate from it rather than a branch inside it
//! because the two questions barely overlap: that one asks for a name and
//! guards one collision, this one asks for a folder and guards a set.
//!
//! What happens *after* the folder is settled is [`super::worker`]'s.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, AlertDialog, ApplicationWindow, FileDialog};

use super::super::SPLIT;
use super::{options, worker, SplitRequest};
use crate::app::i18n::{tr, tr_args};
use crate::app::state::Viewer;

/// Asks which folder the parts go into.
pub(super) fn choose_folder(window: &ApplicationWindow, viewer: &Viewer, request: SplitRequest) {
    let chooser = FileDialog::builder()
        .title(SPLIT.title())
        .accept_label(tr("split-accept"))
        .build();
    // Held until the answer is in, and bound to the tab it is for —
    // see `tabs::ChooserScope`.
    let scope = crate::app::tabs::ChooserScope::for_active(viewer);
    chooser.select_folder(Some(window), None::<&gio::Cancellable>, {
        let window = window.clone();
        let viewer = viewer.clone();
        move |result| {
            if !scope.settle(&viewer) {
                return;
            }
            let Ok(folder) = result else {
                viewer.info(&SPLIT.cancelled());
                return;
            };
            let Some(folder) = folder.path() else {
                viewer.refuse(&tr("split-not-local-folder"));
                return;
            };
            confirm_overwrites(&window, &viewer, request, folder);
        }
    });
}

/// Guards the whole set of writes behind one prompt.
///
/// `chooser::confirm_destination` asks per file, which is right when there is
/// one; asking it four times in a row for a four-way split would be four
/// dialogs for a single decision. So the collisions are counted first and the
/// question is asked once — and asked at all, because a folder chooser raises
/// no overwrite warning of its own: the folder exists, and the names inside
/// it were never typed by the user to be warned about.
///
/// Counting them means `try_exists` per part before anything is written, on
/// the main thread. That is one `stat` per file against a folder the user has
/// just browsed to, and it has to happen before the worker starts or the
/// prompt would be a question asked after the answer.
fn confirm_overwrites(
    window: &ApplicationWindow,
    viewer: &Viewer,
    request: SplitRequest,
    folder: PathBuf,
) {
    let total = request.parts.len();
    if part_held_by_other_tab(viewer, &folder, &request.stem, total) {
        viewer.refuse(&tr("tabs-path-open-elsewhere"));
        return;
    }
    let mut existing = 0;
    for index in 0..total {
        let name = options::part_file_name(&request.stem, index, total);
        match folder.join(&name).try_exists() {
            Ok(true) => existing += 1,
            Ok(false) => {}
            Err(error) => {
                viewer.fail(&tr_args(
                    "split-check-failed",
                    &[
                        ("name", name.as_str().into()),
                        ("error", error.to_string().into()),
                    ],
                ));
                return;
            }
        }
    }
    if existing == 0 {
        worker::spawn_split(viewer, request, folder);
        return;
    }

    let dialog = AlertDialog::builder()
        .message(tr_args(
            "split-replace-title",
            &[("count", existing.into())],
        ))
        .detail(tr_args(
            "split-replace-detail",
            &[("total", total.into()), ("count", existing.into())],
        ))
        .buttons(vec![tr("split-cancel"), tr("split-replace")])
        .cancel_button(0)
        .default_button(0)
        .modal(true)
        .build();
    // The request is not `Clone` — it carries a whole document model — and
    // `AlertDialog::choose` wants a callback it may hold. Handing it over
    // through a `Cell` keeps the single ownership honest: whichever call runs
    // takes it, and a second one would find `None` rather than a stale copy.
    let pending = Rc::new(Cell::new(Some((request, folder))));
    dialog.choose(Some(window), None::<&gio::Cancellable>, {
        let viewer = viewer.clone();
        move |response| match (response == Ok(1), pending.take()) {
            // Asked again now: a tab may have opened one of the parts while
            // the question was up.
            (true, Some((request, folder)))
                if part_held_by_other_tab(&viewer, &folder, &request.stem, request.parts.len()) =>
            {
                viewer.refuse(&tr("tabs-path-open-elsewhere"))
            }
            (true, Some((request, folder))) => worker::spawn_split(&viewer, request, folder),
            (true, None) => {}
            (false, _) => viewer.info(&SPLIT.cancelled()),
        }
    });
}

/// Whether one of the `total` parts named after `stem` would land on a file
/// another tab has open — which would leave that tab showing, and saving,
/// bytes the file no longer holds. Refused like a Save As onto it.
fn part_held_by_other_tab(viewer: &Viewer, folder: &Path, stem: &str, total: usize) -> bool {
    (0..total).any(|index| {
        let name = options::part_file_name(stem, index, total);
        crate::app::tabs::path_held_by_other_tab(viewer, &folder.join(name))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::document::SAMPLE_PDF;
    use crate::app::state::DocumentSource;
    use crate::app::ui_tests::built_ui;

    /// A part file another tab opens while the Replace question is up is
    /// refused when the answer comes, not only before the question.
    #[gtk::test]
    fn gtk_ui_a_part_opened_elsewhere_during_the_replace_question_is_refused() {
        let built = built_ui();
        let folder = std::env::temp_dir().join(format!("vitela-split-late-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let source = folder.join("source.pdf");
        std::fs::write(&source, SAMPLE_PDF).unwrap();
        for index in 0..2 {
            std::fs::write(
                folder.join(options::part_file_name("report", index, 2)),
                SAMPLE_PDF,
            )
            .unwrap();
        }
        let (base, _security) = pdf_manip::open_document(&source, None).expect("it opens");
        let document = pdf_save::document_from_lopdf(&base, None).expect("it models");
        let request = SplitRequest {
            document,
            backing: crate::app::state::SaveBacking {
                base: base.into(),
                original_bytes: SAMPLE_PDF.to_vec().into(),
                password: None,
            },
            sources: Vec::new(),
            parts: vec![(0, 0), (1, 2)],
            stem: "report".to_owned(),
            signed: false,
        };
        crate::app::tabs::open_and_wait(
            &built.window,
            &built.viewer,
            DocumentSource::Embedded(SAMPLE_PDF),
        );

        confirm_overwrites(&built.window, &built.viewer, request, folder.clone());
        let dialogs = || -> Vec<gtk::Window> {
            gtk::Window::list_toplevels()
                .into_iter()
                .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
                .filter(|window| {
                    window.transient_for().as_ref() == Some(built.window.upcast_ref())
                        && window.is_visible()
                })
                .collect()
        };
        assert!(crate::app::test_support::wait_until(|| dialogs().len() == 1));
        crate::app::document::open_initial(
            &built.window,
            &built.viewer,
            DocumentSource::File(folder.join(options::part_file_name("report", 0, 2))),
        );
        assert!(crate::app::test_support::wait_until(|| {
            built.viewer.tabs.borrow().parked_count() == 1
        }));
        fn find(root: &gtk::Widget, label: &str) -> Option<gtk::Button> {
            if let Some(button) = root.downcast_ref::<gtk::Button>() {
                if button.label().as_deref() == Some(label) {
                    return Some(button.clone());
                }
            }
            let mut child = root.first_child();
            while let Some(widget) = child {
                if let Some(found) = find(&widget, label) {
                    return Some(found);
                }
                child = widget.next_sibling();
            }
            None
        }
        let replace = dialogs()
            .iter()
            .find_map(|dialog| find(dialog.upcast_ref(), &tr("split-replace")))
            .expect("the Replace button");

        replace.emit_clicked();
        crate::app::test_support::pump();
        let status = built.viewer.last_said().to_string();

        for dialog in dialogs() {
            dialog.destroy();
        }
        built.viewer.state.borrow_mut().session = None;
        built.viewer.tabs.borrow_mut().clear_for_test();
        built.window.close();
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(status, tr("tabs-path-open-elsewhere"));
    }

    #[gtk::test]
    fn gtk_ui_a_part_landing_on_a_file_another_tab_holds_is_refused() {
        let built = built_ui();
        let folder = std::env::temp_dir().join(format!("vitela-split-held-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let held = folder.join(options::part_file_name("report", 1, 2));
        std::fs::write(&held, SAMPLE_PDF).unwrap();
        crate::app::tabs::open_and_wait(&built.window, &built.viewer, DocumentSource::File(held));
        crate::app::tabs::open_and_wait(
            &built.window,
            &built.viewer,
            DocumentSource::Embedded(SAMPLE_PDF),
        );

        let refused = part_held_by_other_tab(&built.viewer, &folder, "report", 2);
        let other_stem = part_held_by_other_tab(&built.viewer, &folder, "minutes", 2);

        built.viewer.state.borrow_mut().session = None;
        built.viewer.tabs.borrow_mut().clear_for_test();
        built.window.close();
        let _ = std::fs::remove_dir_all(&folder);
        assert!(refused);
        assert!(!other_stem);
    }
}
