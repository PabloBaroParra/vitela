//! Asking the user where a write should go, and whether it may proceed.
//!
//! Everything the three destination-taking chains do *before* a byte is
//! produced: the file chooser, the "Replace existing PDF?" guard, and the
//! warning a signed document earns. One copy of each, parameterised by the
//! [`WriteOperation`] whose words it is speaking in.
//!
//! What happens *after* the destination is settled is [`super::worker`]'s.

use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, AlertDialog, ApplicationWindow, FileDialog};

use super::super::state::Viewer;
use super::{pdf_filter, WriteOperation};

/// What [`choose_destination`] and [`confirm_destination`] hand a confirmed
/// destination to.
///
/// `Rc` rather than a generic `impl Fn`, because the replace guard has to
/// call it from inside a `'static` GTK callback *and* the chooser keeps a
/// copy for the branch that never reaches the guard. The window travels with
/// it: an ordinary save still has a prompt to raise past this point, and the
/// two chains that do not simply ignore it.
pub(super) type Proceed = Rc<dyn Fn(&ApplicationWindow, &Viewer, PathBuf)>;

/// Asks where to write, then hands the answer to `proceed` through the
/// overwrite guard.
///
/// This shell has no notion of "the current file" to overwrite implicitly —
/// not for an ordinary Save, not for signing, not for protecting — so every
/// write to disk starts here.
pub(super) fn choose_destination(
    window: &ApplicationWindow,
    viewer: &Viewer,
    operation: WriteOperation,
    proceed: Proceed,
) {
    let filter = pdf_filter();
    let chooser = FileDialog::builder()
        .title(operation.title)
        .accept_label("Save")
        .default_filter(&filter)
        .initial_name("document.pdf")
        .build();
    chooser.save(Some(window), None::<&gio::Cancellable>, {
        let window = window.clone();
        let viewer = viewer.clone();
        move |result| {
            let Ok(file) = result else {
                viewer.status.set_text(operation.cancelled);
                return;
            };
            let Some(path) = file.path() else {
                viewer
                    .status
                    .set_text("The selected location is not a local file.");
                return;
            };
            confirm_destination(
                &window,
                &viewer,
                operation,
                pdf_destination(path),
                proceed.clone(),
            );
        }
    });
}

fn pdf_destination(mut path: PathBuf) -> PathBuf {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        path.set_extension("pdf");
    }
    path
}

/// Guards every overwrite behind the same prompt.
///
/// The native chooser raises its own overwrite warning, but only for the name
/// the user typed — appending `.pdf` can land on a file it never checked. One
/// prompt here covers both cases, so a collision is always a question rather
/// than sometimes a dead end the user has to reopen the chooser to escape.
fn confirm_destination(
    window: &ApplicationWindow,
    viewer: &Viewer,
    operation: WriteOperation,
    destination: PathBuf,
    proceed: Proceed,
) {
    match destination.try_exists() {
        Ok(false) => proceed(window, viewer, destination),
        Err(error) => viewer.status.set_text(&format!(
            "Could not check whether {} already exists: {error}",
            destination.display()
        )),
        Ok(true) => {
            let dialog = AlertDialog::builder()
                .message("Replace existing PDF?")
                .buttons(["Cancel", "Replace"])
                .cancel_button(0)
                .default_button(1)
                .modal(true)
                .build();
            dialog.choose(Some(window), None::<&gio::Cancellable>, {
                let viewer = viewer.clone();
                let window = window.clone();
                move |response| {
                    if response == Ok(1) {
                        proceed(&window, &viewer, destination.clone());
                    } else {
                        viewer.status.set_text(operation.cancelled);
                    }
                }
            });
        }
    }
}

/// Saving a signed document breaks its signature, and there is no version of
/// this operation that does not: a rewrite replaces the bytes the signature
/// covers. `pdf-save` refuses such a save unless the caller states the user
/// was told, which is what this prompt is for — the core makes sure the
/// question gets asked, and the answer stays the user's.
///
/// The wording says the signature is *invalidated*, not removed, because that
/// is what happens: nothing in `pdf-save`'s full rewrite strips `/Sig`,
/// `/FT /Sig` or `/AcroForm /SigFlags`, so the saved file still carries the
/// signature and a reader opening it reports it as **invalid**, not absent.
/// Those are different outcomes for someone about to send the file on — one
/// is an unsigned document, the other looks tampered with — and the text has
/// to say which one they are choosing. Keep this and
/// `MainWindow.AskSignatureLossAsync` in the same words.
///
/// `operation` supplies the word for backing out, and `proceed` is what runs
/// when the user accepts — the prompt itself is the same question for an
/// ordinary save and for applying protection, because both reach the
/// rewriter and the rewriter is what breaks the signature.
pub(super) fn confirm_signature_loss(
    window: &ApplicationWindow,
    viewer: &Viewer,
    operation: WriteOperation,
    proceed: Rc<dyn Fn()>,
) {
    let dialog = AlertDialog::builder()
        .message("Saving will break this document's signature")
        .detail(
            "This document is signed. Saving rewrites the file, so the signature \
             will no longer match what it covers.\n\n\
             It is not removed: the saved file still carries the signature, and \
             PDF readers will report it as invalid rather than missing.\n\n\
             To keep a copy that still verifies, cancel and save to a different \
             file.",
        )
        .buttons(["Cancel", "Save anyway"])
        .cancel_button(0)
        .default_button(0)
        .modal(true)
        .build();

    dialog.choose(Some(window), None::<&gio::Cancellable>, {
        let viewer = viewer.clone();
        move |response| {
            if response == Ok(1) {
                proceed();
            } else {
                viewer.status.set_text(operation.cancelled);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::pdf_destination;
    use std::path::PathBuf;

    #[test]
    fn a_destination_without_an_extension_gets_a_pdf_suffix() {
        assert_eq!(
            pdf_destination(PathBuf::from("signed-document")),
            PathBuf::from("signed-document.pdf")
        );
    }

    #[test]
    fn a_non_pdf_destination_extension_is_replaced() {
        assert_eq!(
            pdf_destination(PathBuf::from("preview.png")),
            PathBuf::from("preview.pdf")
        );
    }

    #[test]
    fn an_existing_pdf_suffix_is_preserved() {
        assert_eq!(
            pdf_destination(PathBuf::from("signed.PDF")),
            PathBuf::from("signed.PDF")
        );
    }
}
