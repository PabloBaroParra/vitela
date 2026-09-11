use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{
    gio, glib, AlertDialog, ApplicationWindow, Box as GtkBox, Button, FileDialog, FileFilter,
    Label, Orientation, PasswordEntry, Window,
};
use pdf_document::{Command, ImportedDocumentId, Page};

use crate::app::state::{ImportedSource, SessionToken, Viewer};

use super::command::{apply_command, command, model};
use super::grid::populate_grid;
use super::NO_DOCUMENT;

const IMPORT_CANCELLED: &str = "PDF import cancelled.";

#[derive(Clone)]
struct PreparedImport {
    sources: Vec<ImportedSource>,
    pages: Vec<Page>,
    warnings: Vec<String>,
}

struct ImportRequest {
    paths: Vec<PathBuf>,
    passwords: HashMap<PathBuf, String>,
    token: SessionToken,
    first_source_id: u64,
    first_page_id: u32,
    cancellation: Arc<AtomicBool>,
}

enum PrepareError {
    Cancelled,
    PasswordRequired { path: PathBuf, name: String },
    Failed(String),
}

#[derive(Clone, Copy)]
struct ImportProgress {
    completed: usize,
    total: usize,
}

pub(super) fn show_chooser(window: &ApplicationWindow, viewer: &Viewer) {
    if let Some(refusal) = import_refusal(viewer) {
        viewer.status.set_text(refusal);
        return;
    }
    let filter = FileFilter::new();
    filter.set_name(Some("PDF files"));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");
    filter.add_pattern("*.PDF");
    let chooser = FileDialog::builder()
        .title("Add PDFs")
        .accept_label("Add")
        .default_filter(&filter)
        .build();
    chooser.open_multiple(Some(window), None::<&gio::Cancellable>, {
        let window = window.clone();
        let viewer = viewer.clone();
        move |result| {
            let Ok(files) = result else {
                viewer.status.set_text(IMPORT_CANCELLED);
                return;
            };
            let paths: Result<Vec<_>, _> = (0..files.n_items())
                .map(|index| {
                    files
                        .item(index)
                        .and_then(|item| item.downcast::<gio::File>().ok())
                        .and_then(|file| file.path())
                        .ok_or("Every selected PDF must be a local file.")
                })
                .collect();
            match paths {
                Ok(paths) if !paths.is_empty() => start(window.clone(), viewer.clone(), paths),
                Ok(_) => viewer.status.set_text(IMPORT_CANCELLED),
                Err(error) => viewer.status.set_text(error),
            }
        }
    });
}

pub(super) fn cancel(viewer: &Viewer) {
    let (cancellation, dialog) = {
        let mut state = viewer.state.borrow_mut();
        (
            state.import_cancellation.take(),
            state.import_password_dialog.take(),
        )
    };
    if let Some(cancellation) = cancellation {
        cancellation.store(true, Ordering::Release);
    }
    if let Some(dialog) = dialog {
        dialog.destroy();
    }
    finish(viewer);
    viewer.status.set_text(IMPORT_CANCELLED);
}

fn import_refusal(viewer: &Viewer) -> Option<&'static str> {
    viewer
        .content_edit_refusal()
        .or_else(|| viewer.page_assembly_refusal())
        .or_else(|| viewer.full_rewrite_refusal())
}

fn start(window: ApplicationWindow, viewer: Viewer, paths: Vec<PathBuf>) {
    let Some((token, first_source_id, first_page_id)) = import_ids(&viewer) else {
        viewer.status.set_text(NO_DOCUMENT);
        return;
    };
    let cancellation = Arc::new(AtomicBool::new(false));
    let (previous, previous_dialog) = {
        let mut state = viewer.state.borrow_mut();
        (
            state.import_cancellation.replace(cancellation.clone()),
            state.import_password_dialog.take(),
        )
    };
    if let Some(previous) = previous {
        previous.store(true, Ordering::Release);
    }
    if let Some(dialog) = previous_dialog {
        dialog.destroy();
    }
    set_running(&viewer, paths.len());
    run(
        window,
        viewer,
        ImportRequest {
            paths,
            passwords: HashMap::new(),
            token,
            first_source_id,
            first_page_id,
            cancellation,
        },
    );
}

fn run(window: ApplicationWindow, viewer: Viewer, request: ImportRequest) {
    let (sender, receiver) = std::sync::mpsc::channel();
    let paths = request.paths.clone();
    let passwords = request.passwords.clone();
    let first_source_id = request.first_source_id;
    let first_page_id = request.first_page_id;
    let cancellation = request.cancellation.clone();
    let worker = gio::spawn_blocking(move || {
        prepare(
            paths,
            &passwords,
            first_source_id,
            first_page_id,
            &cancellation,
            |progress| {
                let _ = sender.send(progress);
            },
        )
    });
    let progress_source = glib::timeout_add_local(std::time::Duration::from_millis(50), {
        let viewer = viewer.clone();
        move || {
            for progress in receiver.try_iter() {
                update_progress(&viewer, progress);
            }
            if viewer.state.borrow().import_cancellation.is_some() {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        }
    });
    glib::spawn_future_local(async move {
        let result = worker.await;
        progress_source.remove();
        match result {
            Ok(Ok(prepared)) if current(&viewer, &request) => {
                finish(&viewer);
                if prepared.warnings.is_empty() {
                    apply(&viewer, request.token, prepared);
                } else {
                    confirm(&window, &viewer, request.token, prepared);
                }
            }
            Ok(Err(PrepareError::PasswordRequired { path, name }))
                if current(&viewer, &request) =>
            {
                prompt_for_password(&window, &viewer, request, path, name);
            }
            Ok(Err(PrepareError::Cancelled)) => cancelled(&viewer),
            Ok(Err(PrepareError::Failed(error))) if current(&viewer, &request) => {
                finish(&viewer);
                viewer.status.set_text(&error);
            }
            Err(_) if current(&viewer, &request) => {
                finish(&viewer);
                viewer
                    .status
                    .set_text("PDF import worker stopped unexpectedly.");
            }
            _ => finish_if_owned(&viewer, &request.cancellation),
        }
    });
}

fn import_ids(viewer: &Viewer) -> Option<(SessionToken, u64, u32)> {
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    let document = session.document_model.as_ref()?;
    let next_source_id = session
        .imported_sources
        .iter()
        .map(|source| source.id.0)
        .max()
        .map_or(0, |id| id.saturating_add(1));
    let next_page_id = document
        .pages
        .iter()
        .map(|page| page.id.0)
        .max()
        .map_or(0, |id| id.saturating_add(1));
    Some((
        SessionToken {
            generation: state.generation,
            edit_revision: session.edit_revision,
        },
        next_source_id,
        next_page_id,
    ))
}

fn prepare(
    paths: Vec<PathBuf>,
    passwords: &HashMap<PathBuf, String>,
    first_source_id: u64,
    first_page_id: u32,
    cancellation: &AtomicBool,
    mut report: impl FnMut(ImportProgress),
) -> Result<PreparedImport, PrepareError> {
    let total = paths.len();
    let mut sources = Vec::with_capacity(total);
    let mut pages = Vec::new();
    let mut warnings = Vec::new();
    let mut next_page_id = first_page_id;

    report(ImportProgress {
        completed: 0,
        total,
    });
    for (offset, path) in paths.into_iter().enumerate() {
        check_cancelled(cancellation)?;
        let source_id = first_source_id
            .checked_add(offset as u64)
            .ok_or_else(|| failed("Imported PDF id space is exhausted."))?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Selected PDF")
            .to_owned();
        let bytes = std::fs::read(&path)
            .map_err(|error| failed(format!("Could not import {name}: {error}")))?;
        check_cancelled(cancellation)?;
        let password = passwords.get(&path).map(String::as_str);
        // The source's own copy permission is the core's rule, not this
        // shell's: `open_import_source_from_bytes` refuses a PDF that forbids
        // extracting its content before it decrypts a single page.
        let (document, _) = match pdf_manip::open_import_source_from_bytes(&bytes, password) {
            Ok(opened) => opened,
            Err(pdf_manip::ManipError::WrongPassword | pdf_manip::ManipError::PasswordRequired) => {
                return Err(PrepareError::PasswordRequired { path, name });
            }
            Err(pdf_manip::ManipError::SourceForbidsCopying) => {
                return Err(failed(format!(
                    "Could not import {name}: the PDF does not permit copying its pages."
                )));
            }
            Err(error) => return Err(failed(format!("Could not import {name}: {error}"))),
        };
        check_cancelled(cancellation)?;
        let selected: Vec<usize> = (0..document.page_count()).collect();
        if selected.is_empty() {
            return Err(failed(format!(
                "Could not import {name}: the PDF has no pages."
            )));
        }
        let graft = pdf_manip::graft_report(&document, &selected)
            .map_err(|error| failed(format!("Could not import {name}: {error}")))?;
        warnings.extend(
            graft
                .warnings()
                .iter()
                .map(|warning| format!("{name}: {warning}")),
        );
        check_cancelled(cancellation)?;
        let imported = pdf_save::imported_pages_from_lopdf(
            &document,
            ImportedDocumentId(source_id),
            next_page_id,
        )
        .map_err(|error| failed(format!("Could not import {name}: {error}")))?;
        let imported_count = u32::try_from(imported.len())
            .map_err(|_| failed("The selected PDFs have too many pages."))?;
        next_page_id = next_page_id
            .checked_add(imported_count)
            .ok_or_else(|| failed("Page id space is exhausted."))?;
        pages.extend(imported);
        sources.push(ImportedSource {
            id: ImportedDocumentId(source_id),
            document,
            name,
        });
        report(ImportProgress {
            completed: offset + 1,
            total,
        });
    }
    Ok(PreparedImport {
        sources,
        pages,
        warnings,
    })
}

fn check_cancelled(cancellation: &AtomicBool) -> Result<(), PrepareError> {
    if cancellation.load(Ordering::Acquire) {
        Err(PrepareError::Cancelled)
    } else {
        Ok(())
    }
}

fn failed(message: impl Into<String>) -> PrepareError {
    PrepareError::Failed(message.into())
}

fn current(viewer: &Viewer, request: &ImportRequest) -> bool {
    !request.cancellation.load(Ordering::Acquire)
        && viewer
            .state
            .borrow()
            .import_cancellation
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, &request.cancellation))
        && token_matches(viewer, request.token)
}

fn set_running(viewer: &Viewer, total: usize) {
    viewer.organize.add_pdfs_button.set_sensitive(false);
    viewer.organize.import_progress.set_fraction(0.0);
    viewer
        .organize
        .import_progress
        .set_text(Some(&progress_text(ImportProgress {
            completed: 0,
            total,
        })));
    viewer.organize.import_progress.set_show_text(true);
    viewer.organize.import_progress.set_visible(true);
    viewer.organize.cancel_import_button.set_visible(true);
    viewer.status.set_text("Checking selected PDFs...");
}

fn update_progress(viewer: &Viewer, progress: ImportProgress) {
    if viewer.state.borrow().import_cancellation.is_none() {
        return;
    }
    let fraction = if progress.total == 0 {
        0.0
    } else {
        progress.completed as f64 / progress.total as f64
    };
    viewer.organize.import_progress.set_fraction(fraction);
    viewer
        .organize
        .import_progress
        .set_text(Some(&progress_text(progress)));
}

fn progress_text(progress: ImportProgress) -> String {
    format!("{} of {} PDFs", progress.completed, progress.total)
}

fn finish(viewer: &Viewer) {
    viewer.state.borrow_mut().import_cancellation = None;
    viewer.organize.add_pdfs_button.set_sensitive(true);
    viewer.organize.import_progress.set_visible(false);
    viewer.organize.cancel_import_button.set_visible(false);
}

pub(super) fn document_changed(viewer: &Viewer) {
    let (cancellation, dialog) = {
        let mut state = viewer.state.borrow_mut();
        (
            state.import_cancellation.take(),
            state.import_password_dialog.take(),
        )
    };
    if let Some(cancellation) = cancellation {
        cancellation.store(true, Ordering::Release);
    }
    if let Some(dialog) = dialog {
        dialog.destroy();
    }
    viewer.organize.add_pdfs_button.set_sensitive(true);
    viewer.organize.import_progress.set_visible(false);
    viewer.organize.cancel_import_button.set_visible(false);
}

fn finish_if_owned(viewer: &Viewer, cancellation: &Arc<AtomicBool>) {
    let owned = viewer
        .state
        .borrow()
        .import_cancellation
        .as_ref()
        .is_some_and(|active| Arc::ptr_eq(active, cancellation));
    if owned {
        finish(viewer);
    }
}

fn cancelled(viewer: &Viewer) {
    finish(viewer);
    viewer.status.set_text(IMPORT_CANCELLED);
}

fn prompt_for_password(
    window: &ApplicationWindow,
    viewer: &Viewer,
    request: ImportRequest,
    path: PathBuf,
    name: String,
) {
    let content = GtkBox::new(Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let dialog = Window::builder()
        .transient_for(window)
        .modal(true)
        .title("Password required")
        .child(&content)
        .build();
    let prompt = Label::new(Some(&format!("Enter the password for {name}.")));
    prompt.set_xalign(0.0);
    let password_entry = PasswordEntry::builder().show_peek_icon(true).build();
    let error = Label::new(None);
    error.set_xalign(0.0);
    if request.passwords.contains_key(&path) {
        error.set_text("The password is incorrect. Try again.");
    }
    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    let cancel_button = Button::with_label("Cancel import");
    let unlock = Button::with_label("Unlock");
    buttons.append(&cancel_button);
    buttons.append(&unlock);
    content.append(&prompt);
    content.append(&password_entry);
    content.append(&error);
    content.append(&buttons);
    password_entry.grab_focus();
    viewer.state.borrow_mut().import_password_dialog = Some(dialog.clone());
    viewer
        .status
        .set_text(&format!("Waiting for the password for {name}."));

    let request = Rc::new(std::cell::RefCell::new(Some(request)));
    let submit: Rc<dyn Fn()> = Rc::new({
        let dialog = dialog.clone();
        let window = window.clone();
        let viewer = viewer.clone();
        let password_entry = password_entry.clone();
        let request = request.clone();
        move || {
            let Some(mut request) = request.borrow_mut().take() else {
                return;
            };
            request
                .passwords
                .insert(path.clone(), password_entry.text().to_string());
            dismiss_password_dialog(&viewer, &dialog);
            viewer.status.set_text("Checking selected PDFs...");
            run(window.clone(), viewer.clone(), request);
        }
    });
    unlock.connect_clicked({
        let submit = submit.clone();
        move |_| submit()
    });
    password_entry.connect_activate(move |_| submit());
    cancel_button.connect_clicked({
        let viewer = viewer.clone();
        move |_| cancel(&viewer)
    });
    dialog.present();
}

fn dismiss_password_dialog(viewer: &Viewer, dialog: &Window) {
    let mut state = viewer.state.borrow_mut();
    if state.import_password_dialog.as_ref() == Some(dialog) {
        state.import_password_dialog = None;
    }
    drop(state);
    dialog.destroy();
}

fn confirm(
    window: &ApplicationWindow,
    viewer: &Viewer,
    token: SessionToken,
    prepared: PreparedImport,
) {
    let detail = prepared.warnings.join("\n");
    let dialog = AlertDialog::builder()
        .message("Some document-level information will stay behind")
        .detail(&detail)
        .buttons(["Cancel", "Import anyway"])
        .cancel_button(0)
        .default_button(0)
        .modal(true)
        .build();
    dialog.choose(Some(window), None::<&gio::Cancellable>, {
        let viewer = viewer.clone();
        move |response| {
            if response == Ok(1) {
                apply(&viewer, token, prepared.clone());
            } else {
                viewer.status.set_text(IMPORT_CANCELLED);
            }
        }
    });
}

fn token_matches(viewer: &Viewer, token: SessionToken) -> bool {
    let state = viewer.state.borrow();
    state
        .session
        .as_ref()
        .is_some_and(|session| token.matches(state.generation, session.edit_revision))
}

fn apply(viewer: &Viewer, token: SessionToken, prepared: PreparedImport) {
    if !token_matches(viewer, token) {
        viewer
            .status
            .set_text("PDF import cancelled because the document changed.");
        return;
    }
    let count = prepared.pages.len();
    let result = command(viewer, |session| {
        let index = model(session)?.pages.len();
        if !apply_command(
            model(session)?,
            Command::ImportPages {
                index,
                pages: prepared.pages,
            },
        ) {
            return Err("Could not add the selected PDFs.".to_string());
        }
        session.imported_sources.extend(prepared.sources);
        Ok(format!("Imported {count} pages."))
    });
    if result {
        populate_grid(viewer);
    }
}

#[cfg(test)]
pub(super) fn apply_prepared(
    viewer: &Viewer,
    token: SessionToken,
    sources: Vec<ImportedSource>,
    pages: Vec<Page>,
) {
    apply(
        viewer,
        token,
        PreparedImport {
            sources,
            pages,
            warnings: Vec::new(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const AES128: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/encrypted/aes_128_user_and_owner.pdf"
    ));
    const RC4_128: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/encrypted/rc4_128_user_and_owner.pdf"
    ));

    fn fixture(name: &str, bytes: &[u8]) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("pdf-import-{}-{name}.pdf", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn cancelled_prepare_stops_before_reading_a_source() {
        let cancellation = AtomicBool::new(true);
        let result = prepare(
            vec![PathBuf::from("missing.pdf")],
            &HashMap::new(),
            0,
            0,
            &cancellation,
            |_| {},
        );

        assert!(matches!(result, Err(PrepareError::Cancelled)));
    }

    #[test]
    fn prepare_reports_the_specific_source_that_needs_a_password() {
        let path = fixture("password-required", AES128);
        let result = prepare(
            vec![path.clone()],
            &HashMap::new(),
            0,
            0,
            &AtomicBool::new(false),
            |_| {},
        );
        std::fs::remove_file(&path).unwrap();

        assert!(matches!(
            result,
            Err(PrepareError::PasswordRequired { path: blocked, .. }) if blocked == path
        ));
    }

    #[test]
    fn prepare_uses_an_independent_password_for_each_source() {
        let aes = fixture("aes", AES128);
        let rc4 = fixture("rc4", RC4_128);
        let passwords = HashMap::from([
            (aes.clone(), "user-aes-pass".to_string()),
            (rc4.clone(), "user-rc4-pass".to_string()),
        ]);
        let mut progress = Vec::new();
        let result = prepare(
            vec![aes.clone(), rc4.clone()],
            &passwords,
            0,
            0,
            &AtomicBool::new(false),
            |update| progress.push((update.completed, update.total)),
        );
        std::fs::remove_file(aes).unwrap();
        std::fs::remove_file(rc4).unwrap();

        assert!(
            result.is_ok(),
            "independent source passwords must unlock both PDFs"
        );
        assert_eq!(progress, vec![(0, 2), (1, 2), (2, 2)]);
    }

    #[test]
    fn prepare_rejects_a_wrong_password_for_only_its_source() {
        let aes = fixture("wrong-aes", AES128);
        let rc4 = fixture("right-rc4", RC4_128);
        let passwords = HashMap::from([
            (aes.clone(), "wrong".to_string()),
            (rc4.clone(), "user-rc4-pass".to_string()),
        ]);
        let result = prepare(
            vec![aes.clone(), rc4.clone()],
            &passwords,
            0,
            0,
            &AtomicBool::new(false),
            |_| {},
        );
        std::fs::remove_file(&aes).unwrap();
        std::fs::remove_file(rc4).unwrap();

        assert!(matches!(
            result,
            Err(PrepareError::PasswordRequired { path: blocked, .. }) if blocked == aes
        ));
    }

    #[test]
    fn progress_text_names_completed_and_total_sources() {
        assert_eq!(
            progress_text(ImportProgress {
                completed: 3,
                total: 5
            }),
            "3 of 5 PDFs"
        );
    }
}
