//! Asking the user where a write should go, and whether it may proceed.
//!
//! Everything the three destination-taking chains do *before* a byte is
//! produced: the file chooser, the "Replace existing PDF?" guard, and the
//! warning a signed document earns. One copy of each, parameterised by the
//! [`WriteOperation`] whose words it is speaking in.
//!
//! What happens *after* the destination is settled is [`super::worker`]'s.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, AlertDialog, ApplicationWindow, FileDialog};

use super::super::state::Viewer;
use super::{pdf_filter, WriteOperation};
use crate::app::i18n::{tr, tr_args};

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
/// Signing and protecting never overwrite "the current file" implicitly, so
/// both start here. So does every Save As, and an ordinary Save whose
/// document has no file it can write back to (`super::in_place`).
pub(super) fn choose_destination(
    window: &ApplicationWindow,
    viewer: &Viewer,
    operation: WriteOperation,
    proceed: Proceed,
) {
    choose_destination_from(window, viewer, operation, None, proceed);
}

/// [`choose_destination`], opened on `suggested` — the file the document came
/// from — instead of a blank `document.pdf`. Still a question: picking the
/// suggested file goes through the same overwrite guard as any other.
pub(super) fn choose_destination_from(
    window: &ApplicationWindow,
    viewer: &Viewer,
    operation: WriteOperation,
    suggested: Option<&Path>,
    proceed: Proceed,
) {
    pick_destination_unguarded(
        window,
        viewer,
        operation,
        suggested,
        Rc::new(move |window, viewer, picked| {
            confirm_destination(window, viewer, operation, picked, proceed.clone());
        }),
    );
}

/// The chooser alone, opened on `suggested`: hands the path the user picked
/// — a `.pdf` suffix added — to `picked`, with **no** overwrite guard in
/// between, so `picked` must make its own. For a chain whose guard has to
/// ask about more than the picked name (`super::save`); every other caller
/// wants [`choose_destination_from`].
pub(super) fn pick_destination_unguarded(
    window: &ApplicationWindow,
    viewer: &Viewer,
    operation: WriteOperation,
    suggested: Option<&Path>,
    picked: Proceed,
) {
    let filter = pdf_filter();
    let builder = FileDialog::builder()
        .title(operation.title())
        .accept_label(tr("write-accept"))
        .default_filter(&filter);
    let chooser = match suggested {
        Some(path) => builder.initial_file(&gio::File::for_path(path)),
        None => builder.initial_name("document.pdf"),
    }
    .build();
    // Holds the active tab in place while the chooser is up, and binds the
    // answer to it: a write must never take another document's bytes. See
    // `tabs::ChooserScope`.
    let scope = crate::app::tabs::ChooserScope::for_active(viewer);
    chooser.save(Some(window), None::<&gio::Cancellable>, {
        let window = window.clone();
        let viewer = viewer.clone();
        move |result| {
            if !scope.settle(&viewer) {
                return;
            }
            let Ok(file) = result else {
                viewer.info(&operation.cancelled());
                return;
            };
            let Some(path) = file.path() else {
                viewer.refuse(&tr("write-not-local-file"));
                return;
            };
            picked(&window, &viewer, pdf_destination(path));
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

/// Guards every overwrite behind the same prompt — and refuses outright a
/// file another tab has open, which would go on showing, and saving, bytes
/// the file no longer holds.
///
/// The native chooser raises its own overwrite warning, but only for the name
/// the user typed — appending `.pdf` can land on a file it never checked. One
/// prompt here covers both cases, so a collision is always a question rather
/// than sometimes a dead end the user has to reopen the chooser to escape.
pub(super) fn confirm_destination(
    window: &ApplicationWindow,
    viewer: &Viewer,
    operation: WriteOperation,
    destination: PathBuf,
    proceed: Proceed,
) {
    if crate::app::tabs::path_held_by_other_tab(viewer, &destination) {
        viewer.refuse(&tr("tabs-path-open-elsewhere"));
        return;
    }
    match destination.try_exists() {
        Ok(false) => proceed(window, viewer, destination),
        Err(error) => viewer.fail(&check_failed(&destination, &error.to_string())),
        Ok(true) => {
            let dialog = replace_dialog(&tr("write-replace-title"));
            dialog.choose(Some(window), None::<&gio::Cancellable>, {
                let viewer = viewer.clone();
                let window = window.clone();
                move |response| {
                    // Asked again now: a tab may have opened the file while
                    // the question was up.
                    if response == Ok(1)
                        && crate::app::tabs::path_held_by_other_tab(&viewer, &destination)
                    {
                        viewer.refuse(&tr("tabs-path-open-elsewhere"));
                    } else if response == Ok(1) {
                        proceed(&window, &viewer, destination.clone());
                    } else {
                        viewer.info(&operation.cancelled());
                    }
                }
            });
        }
    }
}

/// The "Replace?" question, asked as `message`: Cancel, or Replace (answer
/// `1`, the default).
pub(super) fn replace_dialog(message: &str) -> AlertDialog {
    AlertDialog::builder()
        .message(message)
        .buttons([tr("write-replace-cancel"), tr("write-replace")])
        .cancel_button(0)
        .default_button(1)
        .modal(true)
        .build()
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
        .message(tr("write-signature-title"))
        .detail(tr("write-signature-detail"))
        .buttons([tr("write-signature-cancel"), tr("write-signature-proceed")])
        .cancel_button(0)
        .default_button(0)
        .modal(true)
        .build();

    // The answer is about the document on screen now: accepting it after the
    // active tab changed would write another one, so it is refused.
    let bound = crate::app::tabs::bind_active(viewer);
    dialog.choose(Some(window), None::<&gio::Cancellable>, {
        let viewer = viewer.clone();
        move |response| {
            if response != Ok(1) {
                viewer.info(&operation.cancelled());
            } else if bound.ensure_active_or_report(&viewer) {
                proceed();
            }
        }
    });
}

/// What the toast says for a destination whose existence could not be checked;
/// the file-system `error` stays verbatim.
fn check_failed(destination: &std::path::Path, error: &str) -> String {
    tr_args(
        "write-check-failed",
        &[
            ("path", destination.display().to_string().into()),
            ("error", error.into()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::{check_failed, confirm_destination, pdf_destination};
    use crate::app::document::SAMPLE_PDF;
    use crate::app::state::DocumentSource;
    use crate::app::ui_tests::built_ui;
    use gtk::prelude::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// A Save As onto the file another tab has open is refused before the
    /// overwrite question, and nothing is written.
    #[gtk::test]
    fn gtk_ui_a_destination_another_tab_holds_is_refused() {
        let built = built_ui();
        let held = std::env::temp_dir().join(format!("vitela-held-{}.pdf", std::process::id()));
        std::fs::write(&held, SAMPLE_PDF).unwrap();
        crate::app::tabs::open_and_wait(
            &built.window,
            &built.viewer,
            DocumentSource::File(held.clone()),
        );
        crate::app::tabs::open_and_wait(
            &built.window,
            &built.viewer,
            DocumentSource::Embedded(SAMPLE_PDF),
        );
        let proceeded = Rc::new(Cell::new(false));

        confirm_destination(
            &built.window,
            &built.viewer,
            super::super::SAVE,
            held.clone(),
            Rc::new({
                let proceeded = proceeded.clone();
                move |_, _, _| proceeded.set(true)
            }),
        );

        let status = built.viewer.last_said().to_string();
        built.viewer.state.borrow_mut().session = None;
        built.viewer.tabs.borrow_mut().clear_for_test();
        built.window.close();
        let _ = std::fs::remove_file(&held);
        assert!(!proceeded.get(), "nothing is written");
        assert_eq!(status, tr("tabs-path-open-elsewhere"));
    }

    use crate::app::i18n::{speak, tr, Language};
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

    /// The chooser, replace guard and signature warning as they read before
    /// they moved into the catalogs — byte for byte, including the two blank
    /// lines the signature detail is paragraphed with.
    #[test]
    fn the_shared_prompts_keep_their_english() {
        assert_eq!(tr("write-accept"), "Save");
        assert_eq!(
            tr("write-not-local-file"),
            "The selected location is not a local file."
        );
        assert_eq!(
            check_failed(&PathBuf::from("/tmp/a {b}.pdf"), "denied {x}"),
            "Could not check whether /tmp/a {b}.pdf already exists: denied {x}"
        );
        assert_eq!(tr("write-replace-title"), "Replace existing PDF?");
        assert_eq!(tr("write-replace-cancel"), "Cancel");
        assert_eq!(tr("write-replace"), "Replace");
        assert_eq!(
            tr("write-signature-title"),
            "Saving will break this document's signature"
        );
        assert_eq!(
            tr("write-signature-detail"),
            "This document is signed. Saving rewrites the file, so the signature \
             will no longer match what it covers.\n\n\
             It is not removed: the saved file still carries the signature, and \
             PDF readers will report it as invalid rather than missing.\n\n\
             To keep a copy that still verifies, cancel and save to a different \
             file."
        );
        assert_eq!(tr("write-signature-cancel"), "Cancel");
        assert_eq!(tr("write-signature-proceed"), "Save anyway");
    }

    #[test]
    fn the_shared_prompts_speak_spanish() {
        let _spanish = speak(Language::Spanish);
        assert_eq!(tr("write-accept"), "Guardar");
        assert_eq!(
            tr("write-not-local-file"),
            "La ubicación seleccionada no es un archivo local."
        );
        assert_eq!(
            check_failed(&PathBuf::from("/tmp/a {b}.pdf"), "denied {x}"),
            "No se pudo comprobar si /tmp/a {b}.pdf ya existe: denied {x}"
        );
        assert_eq!(tr("write-replace-title"), "¿Reemplazar el PDF existente?");
        assert_eq!(tr("write-replace-cancel"), "Cancelar");
        assert_eq!(tr("write-replace"), "Reemplazar");
        assert_eq!(
            tr("write-signature-title"),
            "Al guardar se romperá la firma de este documento"
        );
        assert_eq!(
            tr("write-signature-detail"),
            "Este documento está firmado. Al guardar se reescribe el archivo, así que \
             la firma dejará de coincidir con lo que cubre.\n\n\
             No se elimina: el archivo guardado sigue llevando la firma, y los lectores \
             de PDF la mostrarán como no válida, no como ausente.\n\n\
             Para conservar una copia que siga siendo válida, cancela y guarda en otro \
             archivo."
        );
        assert_eq!(tr("write-signature-cancel"), "Cancelar");
        assert_eq!(tr("write-signature-proceed"), "Guardar de todos modos");
    }
}
