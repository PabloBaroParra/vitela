//! Digital-signature identity discovery from a `.pfx`/`.p12` file (Batch
//! B23 Fase 2): a file chooser filtered to PKCS#12 containers, a password
//! prompt mirroring `document::prompt_for_password`'s shape, and
//! `PfxCertificateSource::from_file` reporting the identities it finds.
//!
//! This is deliberately a self-contained slice: nothing downstream of
//! `list_identities` exists yet. Fase 3 adds a PKCS#11 twin of this file's
//! flow, Fase 4 an identity picker that actually calls
//! `pdf_sign::orchestrate::sign_document`, and Fase 5 wires both into the
//! rail's disabled "Sign" button and the "Fill & Sign" tab this module's
//! button lives on. Until then, "pick a `.pfx`, confirm it unlocks and which
//! certificates it holds" is already useful on its own — see
//! `docs/batch-digital-signature.md`.

use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gio, glib, ApplicationWindow, Box as GtkBox, Button, FileDialog, FileFilter, Label,
    Orientation as GtkOrientation, PasswordEntry, Window,
};
use pdf_sign::{CertificateSourcePort, SigningIdentity};
use pdf_sign_pfx::{PfxAdapterError, PfxCertificateSource};

use crate::app::state::Viewer;
use crate::app::tools_panel::panel_heading;

/// Builds the "Fill & Sign" page's signing section: a heading and the
/// "Choose signing certificate" button. `connect_sign_toolbar` wires the
/// button; this only builds the widgets so `mod.rs` can compose them
/// alongside `forms::build_forms_content`'s own section on the same page.
pub(crate) fn build_sign_content() -> (Button, GtkBox) {
    let choose_pfx = Button::with_label("Choose signing certificate (.pfx)…");

    let content = GtkBox::new(GtkOrientation::Vertical, 8);
    content.append(&panel_heading("Signing"));
    content.append(&choose_pfx);

    (choose_pfx, content)
}

pub(crate) fn connect_sign_toolbar(window: &ApplicationWindow, viewer: &Viewer) {
    viewer.choose_signing_certificate.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| show_pfx_chooser(&window, &viewer)
    });
}

/// T-180: a `GtkFileDialog` filtered to `.pfx`/`.p12`, mirroring
/// `document::show_file_chooser`'s shape.
fn show_pfx_chooser(window: &ApplicationWindow, viewer: &Viewer) {
    let filter = FileFilter::new();
    filter.set_name(Some("PKCS#12 certificates"));
    filter.add_pattern("*.pfx");
    filter.add_pattern("*.PFX");
    filter.add_pattern("*.p12");
    filter.add_pattern("*.P12");

    let chooser = FileDialog::builder()
        .title("Choose signing certificate")
        .accept_label("Open")
        .default_filter(&filter)
        .build();
    chooser.open(Some(window), None::<&gio::Cancellable>, {
        let window = window.clone();
        let viewer = viewer.clone();
        move |result| {
            let Ok(file) = result else {
                return;
            };
            let Some(path) = file.path() else {
                viewer
                    .status
                    .set_text("The selected location is not a local file.");
                return;
            };
            prompt_for_pfx_password(&window, &viewer, path);
        }
    });
}

/// T-180/T-181: the password prompt, and the `PfxCertificateSource::from_file`
/// call it gates. Same visual pattern as `document::prompt_for_password` —
/// a modal `Window` with a `PasswordEntry`, an error label, and Cancel/
/// confirm buttons — with the confirm label and copy adjusted for a
/// certificate file's password rather than a document's.
fn prompt_for_pfx_password(window: &ApplicationWindow, viewer: &Viewer, path: PathBuf) {
    let content = GtkBox::new(GtkOrientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let dialog = Window::builder()
        .transient_for(window)
        .modal(true)
        .title("Certificate password required")
        .child(&content)
        .build();

    // Tracked so a later "Choose signing certificate" attempt (which may
    // supersede this one before the background load resolves) can tear this
    // dialog down instead of leaving it stacked underneath a second one —
    // mirrors `document::begin_loading`/`dismiss_password_dialog`.
    let stale_dialog = viewer.state.borrow_mut().pfx_dialog.replace(dialog.clone());
    if let Some(stale_dialog) = stale_dialog {
        stale_dialog.destroy();
    }

    let password_entry = PasswordEntry::builder().show_peek_icon(true).build();
    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    let buttons = GtkBox::new(GtkOrientation::Horizontal, 8);
    let cancel = Button::with_label("Cancel");
    let unlock = Button::with_label("Unlock");
    buttons.append(&cancel);
    buttons.append(&unlock);
    content.append(&password_entry);
    content.append(&error_label);
    content.append(&buttons);
    password_entry.grab_focus();

    let submit: Rc<dyn Fn()> = Rc::new({
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        let password_entry = password_entry.clone();
        let error_label = error_label.clone();
        let path = path.clone();
        move || {
            viewer.status.set_text("Reading certificate file...");
            dialog.set_sensitive(false);
            glib::spawn_future_local({
                let viewer = viewer.clone();
                let dialog = dialog.clone();
                let password_entry = password_entry.clone();
                let error_label = error_label.clone();
                let path = path.clone();
                async move {
                    // A newer "Choose signing certificate" attempt may have
                    // already superseded this dialog (torn down above, on
                    // entry to a later `prompt_for_pfx_password` call) while
                    // this load was in flight. Applying this stale result
                    // would clobber whatever the newer attempt already
                    // reported — same hazard `document::is_current` guards.
                    if !is_pfx_dialog_current(&viewer, &dialog) {
                        return;
                    }
                    let password = password_entry.text().to_string();
                    match load_pfx_in_background(path, password).await {
                        Ok(source) => {
                            viewer
                                .status
                                .set_text(&identities_status_message(&source.list_identities()));
                            dismiss_pfx_dialog(&viewer, &dialog);
                        }
                        Err(PfxAdapterError::Pkcs12(_)) => {
                            dialog.set_sensitive(true);
                            viewer
                                .status
                                .set_text("Waiting for the certificate password.");
                            error_label.set_text(
                                "The password is incorrect, or the file is not a valid \
                                 PKCS#12 certificate.",
                            );
                            password_entry.set_text("");
                            password_entry.grab_focus();
                        }
                        Err(PfxAdapterError::File(message)) => {
                            viewer.status.set_text(&format!(
                                "Could not read the certificate file: {message}"
                            ));
                            dismiss_pfx_dialog(&viewer, &dialog);
                        }
                    }
                }
            });
        }
    });
    unlock.connect_clicked({
        let submit = submit.clone();
        move |_| submit()
    });
    password_entry.connect_activate(move |_| submit());
    cancel.connect_clicked({
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        move |_| {
            viewer.status.set_text("Certificate selection cancelled.");
            dismiss_pfx_dialog(&viewer, &dialog);
        }
    });
    dialog.present();
}

/// Whether `dialog` is still the tracked in-flight PFX password prompt —
/// `false` once a later attempt has superseded it (see
/// `prompt_for_pfx_password`'s teardown-on-entry above) — the signing twin
/// of `document::is_current`.
fn is_pfx_dialog_current(viewer: &Viewer, dialog: &Window) -> bool {
    viewer.state.borrow().pfx_dialog.as_ref() == Some(dialog)
}

/// Clears the tracked PFX dialog and tears it down — but only if it still
/// points at `dialog`, so this cannot clobber a newer dialog's slot. The
/// signing twin of `document::dismiss_password_dialog`.
fn dismiss_pfx_dialog(viewer: &Viewer, dialog: &Window) {
    let mut state = viewer.state.borrow_mut();
    if state.pfx_dialog.as_ref() == Some(dialog) {
        state.pfx_dialog = None;
    }
    drop(state);
    dialog.destroy();
}

async fn load_pfx_in_background(
    path: PathBuf,
    password: String,
) -> Result<PfxCertificateSource, PfxAdapterError> {
    gio::spawn_blocking(move || PfxCertificateSource::from_file(&path, &password))
        .await
        .expect("PFX load task panicked")
}

/// Formats what `PfxCertificateSource::list_identities` found for the status
/// bar — the same place every other open/save outcome in this shell
/// surfaces, not a dedicated widget. Pure so it is testable without a GTK
/// runtime. Nothing is stored past the caller's `set_text`; Fase 4 (the
/// identity picker that actually signs) will need its own state slot for a
/// chosen `CertificateSourcePort`, shaped by whatever Fase 3's PKCS#11
/// source turns out to need too.
fn identities_status_message(identities: &[SigningIdentity]) -> String {
    if identities.is_empty() {
        return "This certificate file has no usable signing identities.".to_owned();
    }
    let names: Vec<&str> = identities
        .iter()
        .map(|identity| identity.display_name.as_str())
        .collect();
    let identity_word = if identities.len() == 1 {
        "identity"
    } else {
        "identities"
    };
    format!(
        "Found {} signing {identity_word}: {}",
        identities.len(),
        names.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(display_name: &str) -> SigningIdentity {
        SigningIdentity {
            id: display_name.to_owned(),
            display_name: display_name.to_owned(),
            certificate_chain_der: vec![vec![0]],
            supported_algorithms: vec![],
        }
    }

    #[test]
    fn empty_identity_list_reports_no_usable_identities() {
        assert_eq!(
            identities_status_message(&[]),
            "This certificate file has no usable signing identities."
        );
    }

    #[test]
    fn one_identity_uses_the_singular_noun() {
        let message = identities_status_message(&[identity("Alice Doe")]);
        assert_eq!(message, "Found 1 signing identity: Alice Doe");
    }

    #[test]
    fn several_identities_use_the_plural_noun_and_are_all_named() {
        let message =
            identities_status_message(&[identity("Alice Doe"), identity("Signing Cert 2")]);
        assert_eq!(
            message,
            "Found 2 signing identities: Alice Doe, Signing Cert 2"
        );
    }

    #[gtk::test]
    fn gtk_ui_choose_signing_certificate_button_has_the_expected_label() {
        let (button, content) = build_sign_content();

        assert_eq!(
            button.label().as_deref(),
            Some("Choose signing certificate (.pfx)…")
        );
        assert!(button.is_sensitive());
        assert!(content.first_child().is_some());
    }

    fn test_application() -> gtk::Application {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let application = gtk::Application::builder()
            .application_id(format!("org.vitela.Pdf.test.sign{sequence}"))
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gio::Cancellable>)
            .expect("the GTK test application must register");
        application
    }

    /// A second "Choose signing certificate" attempt must tear down and stop
    /// tracking the first prompt — the exact hazard `document.rs`'s
    /// `password_dialog`/`is_current` pair exists to prevent, mirrored here
    /// for the PFX password prompt (see the code-review finding this fixes).
    #[gtk::test]
    fn gtk_ui_a_second_prompt_supersedes_and_destroys_the_first() {
        let application = test_application();
        let built = crate::app::build_ui(&application);

        prompt_for_pfx_password(&built.window, &built.viewer, PathBuf::from("first.pfx"));
        let first_dialog = built
            .viewer
            .state
            .borrow()
            .pfx_dialog
            .clone()
            .expect("the first prompt must track its dialog");

        prompt_for_pfx_password(&built.window, &built.viewer, PathBuf::from("second.pfx"));
        let second_dialog = built
            .viewer
            .state
            .borrow()
            .pfx_dialog
            .clone()
            .expect("the second prompt must track its own dialog");

        assert!(first_dialog != second_dialog);
        assert!(is_pfx_dialog_current(&built.viewer, &second_dialog));
        assert!(!is_pfx_dialog_current(&built.viewer, &first_dialog));

        built.window.close();
    }
}
