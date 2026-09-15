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
use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, AlertDialog, ApplicationWindow, FileDialog};

use super::super::SPLIT;
use super::{options, worker, SplitRequest};
use crate::app::state::Viewer;

/// Asks which folder the parts go into.
pub(super) fn choose_folder(window: &ApplicationWindow, viewer: &Viewer, request: SplitRequest) {
    let chooser = FileDialog::builder()
        .title(SPLIT.title)
        .accept_label("Split")
        .build();
    chooser.select_folder(Some(window), None::<&gio::Cancellable>, {
        let window = window.clone();
        let viewer = viewer.clone();
        move |result| {
            let Ok(folder) = result else {
                viewer.status.set_text(SPLIT.cancelled);
                return;
            };
            let Some(folder) = folder.path() else {
                viewer
                    .status
                    .set_text("The selected location is not a local folder.");
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
    let mut existing = 0;
    for index in 0..total {
        let name = options::part_file_name(&request.stem, index, total);
        match folder.join(&name).try_exists() {
            Ok(true) => existing += 1,
            Ok(false) => {}
            Err(error) => {
                viewer.status.set_text(&format!(
                    "Could not check whether {name} already exists: {error}"
                ));
                return;
            }
        }
    }
    if existing == 0 {
        worker::spawn_split(viewer, request, folder);
        return;
    }

    let files = if existing == 1 { "PDF" } else { "PDFs" };
    let dialog = AlertDialog::builder()
        .message(format!("Replace {existing} existing {files}?"))
        .detail(format!(
            "Splitting into this folder writes {total} files named after the \
             document, and {existing} of them already exist there."
        ))
        .buttons(["Cancel", "Replace"])
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
            (true, Some((request, folder))) => worker::spawn_split(&viewer, request, folder),
            (true, None) => {}
            (false, _) => viewer.status.set_text(SPLIT.cancelled),
        }
    });
}
