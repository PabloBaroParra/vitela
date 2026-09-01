//! Digital-signature identity discovery (Batch B23). Fase 2 covers a `.pfx`/
//! `.p12` file: a file chooser filtered to PKCS#12 containers, a password
//! prompt mirroring `document::prompt_for_password`'s shape, and
//! `PfxCertificateSource::from_file` reporting the identities it finds. Fase
//! 3 adds the card/token twin: a short list of typical Linux PKCS#11 module
//! paths tried in order (decision 2 in `docs/batch-digital-signature.md`)
//! before falling back to a manual `.so` file chooser, then a PIN prompt
//! gating `Pkcs11CertificateSource::load`.
//!
//! Fase 4 adds the identity picker: once either flow above unlocks at least
//! one identity, `open_identity_picker` opens automatically and lets the
//! user choose one to sign with. Confirming runs `begin_sign_from_picker`,
//! which gates the action (batch decision 5, and the `unsaved_to_disk` check
//! `SignRequest` itself documents), then hands off to
//! `document::begin_sign` — the destination chooser, background
//! `pdf_sign::sign_document` call, and save→reopen cycle, T-185's half of
//! this batch. Fase 5 (T-186) wires both flows into the rail's "Sign" button
//! (see `shell::build_app_rail`/`app::build_ui`, which switch the tools
//! panel to the "Fill & Sign" tab this module's buttons live on) and gates
//! that button along with `choose_pfx`/`choose_pkcs11` on the same
//! decision-5 criterion `begin_sign_from_picker` already enforces — see
//! `update_sign_controls` below.
//!
//! A later flow adds the NSS shared certificate database twin: the closest
//! thing Linux has to a system certificate store, `~/.pki/nssdb` (what
//! Chrome and most Firefox profiles read software certificates from). Same
//! module-discovery-then-fallback shape as PKCS#11 (`libsoftokn3.so` instead
//! of an OpenSC driver), plus a profile-directory fallback of its own, ending
//! in `pdf_sign_nss::load` rather than `Pkcs11CertificateSource::load`
//! directly — see that crate for why the NSS-specific `unsafe` FFI lives
//! there and not in `pdf-sign-pkcs11`.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{
    gio, glib, ApplicationWindow, Box as GtkBox, Button, CheckButton, FileDialog, FileFilter,
    Label, Orientation as GtkOrientation, PasswordEntry, Window,
};
use pdf_sign::{CertificateSourcePort, SigningIdentity};
use pdf_sign_nss::NssAdapterError;
use pdf_sign_pfx::{PfxAdapterError, PfxCertificateSource};
use pdf_sign_pkcs11::{Pkcs11AdapterError, Pkcs11CertificateSource};

use crate::app::document::{self, SignRequest};
use crate::app::state::{SessionToken, Viewer};
use crate::app::tools_panel::panel_heading;

/// Builds the "Fill & Sign" page's signing section: a heading and the
/// "Choose signing certificate" / "Use card or token" buttons.
/// `connect_sign_toolbar` wires them; this only builds the widgets so
/// `mod.rs` can compose them alongside `forms::build_forms_content`'s own
/// section on the same page.
pub(crate) fn build_sign_content() -> (Button, Button, Button, Label, GtkBox) {
    let choose_pfx = Button::with_label("Choose signing certificate (.pfx)…");
    let choose_pkcs11 = Button::with_label("Use card or token…");
    let choose_nss = Button::with_label("Use a certificate from this computer…");

    // Hidden until `update_sign_controls` finds a signature on the open
    // document — the one place in this shell a user can tell a signing
    // attempt actually landed, since `document::begin_sign`'s status-bar
    // message (T-185) is overwritten by the very next unrelated action.
    let signed_indicator = Label::new(Some("✓ This document is digitally signed."));
    signed_indicator.set_xalign(0.0);
    signed_indicator.add_css_class("signed-indicator");
    signed_indicator.set_visible(false);

    let content = GtkBox::new(GtkOrientation::Vertical, 8);
    content.append(&panel_heading("Signing"));
    content.append(&signed_indicator);
    content.append(&choose_pfx);
    content.append(&choose_pkcs11);
    content.append(&choose_nss);

    (
        choose_pfx,
        choose_pkcs11,
        choose_nss,
        signed_indicator,
        content,
    )
}

/// T-186: refreshes the signing section's own sensitivity — the signing
/// twin of `forms::toolbar::update_forms_controls`'s `refresh_controls`
/// half. Gated on [`signing_refusal`], the same decision-5 criterion
/// `begin_sign_from_picker` already enforces at the end of the flow; this
/// keeps the front door (the certificate/token buttons) from inviting a
/// click that step would refuse anyway. Called from `document::show_document`
/// whenever a document opens, closes, or is reloaded (including the
/// save→reopen cycle a completed signature itself triggers).
pub(crate) fn update_sign_controls(viewer: &Viewer) {
    let enabled = signing_refusal(viewer).is_none();
    viewer.choose_signing_certificate.set_sensitive(enabled);
    viewer.choose_pkcs11_certificate.set_sensitive(enabled);
    viewer.choose_nss_certificate.set_sensitive(enabled);
    viewer
        .signed_indicator
        .set_visible(document_is_signed(viewer));
}

/// Whether the open document (if any) already carries a signature —
/// `pdf_save::has_signatures`'s own structural scan (`/AcroForm /SigFlags` or
/// any `/FT /Sig` object), the same check `document::confirm_signature_loss`
/// asks before a rewrite that would break one.
fn document_is_signed(viewer: &Viewer) -> bool {
    viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.save_backing.as_ref())
        .is_some_and(|backing| pdf_save::has_signatures(backing.base.as_lopdf()))
}

pub(crate) fn connect_sign_toolbar(window: &ApplicationWindow, viewer: &Viewer) {
    viewer.choose_signing_certificate.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| show_pfx_chooser(&window, &viewer)
    });
    viewer.choose_pkcs11_certificate.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| begin_pkcs11_flow(&window, &viewer)
    });
    viewer.choose_nss_certificate.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| begin_nss_flow(&window, &viewer)
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
        let window = window.clone();
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        let password_entry = password_entry.clone();
        let error_label = error_label.clone();
        let path = path.clone();
        move || {
            viewer.status.set_text("Reading certificate file...");
            dialog.set_sensitive(false);
            glib::spawn_future_local({
                let window = window.clone();
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
                            let identities = source.list_identities();
                            viewer.status.set_text(&identities_status_message(
                                &identities,
                                "This certificate file has no usable signing identities.",
                            ));
                            if !identities.is_empty() {
                                open_identity_picker(
                                    &window,
                                    &viewer,
                                    Arc::new(source),
                                    identities,
                                );
                            }
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

/// Formats what a `CertificateSourcePort::list_identities` call found for the
/// status bar — the same place every other open/save outcome in this shell
/// surfaces, not a dedicated widget. Pure so it is testable without a GTK
/// runtime. `empty_message` lets each source (`.pfx` file vs. PKCS#11 token)
/// phrase the empty case in its own terms — a file with no identities and a
/// token that rejected the PIN look the same to this function but need
/// different guidance. Nothing is stored past the caller's `set_text`; Fase 4
/// (the identity picker that actually signs) will need its own state slot
/// for a chosen `CertificateSourcePort`.
fn identities_status_message(identities: &[SigningIdentity], empty_message: &str) -> String {
    if identities.is_empty() {
        empty_message.to_owned()
    } else {
        format_found_identities(identities)
    }
}

/// The non-empty case shared by `identities_status_message` and the PKCS#11
/// PIN flow, which needs it directly: an empty result there means "retry the
/// PIN" rather than "show a message", so it never reaches
/// `identities_status_message`'s empty branch.
fn format_found_identities(identities: &[SigningIdentity]) -> String {
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

/// T-182: typical install paths for the OpenSC PKCS#11 module across the
/// major Linux packaging layouts (Debian/Ubuntu multiarch, Fedora/RHEL/
/// openSUSE `lib64`, and distros that skip the `pkcs11` subdirectory). Tried
/// in order by `find_pkcs11_module` before falling back to a manual file
/// chooser — decision 2 in `docs/batch-digital-signature.md`: someone without
/// certificate experience cannot be expected to know or type this path.
const PKCS11_MODULE_CANDIDATES: &[&str] = &[
    "/usr/lib/x86_64-linux-gnu/pkcs11/opensc-pkcs11.so",
    "/usr/lib/i386-linux-gnu/pkcs11/opensc-pkcs11.so",
    "/usr/lib64/pkcs11/opensc-pkcs11.so",
    "/usr/lib/pkcs11/opensc-pkcs11.so",
    "/usr/lib/opensc-pkcs11.so",
];

/// The first candidate module that exists on disk, or `None` if none do.
/// Only checks existence — a candidate present but unable to initialize
/// still surfaces its real error from `prompt_for_pkcs11_pin`'s load
/// attempt, same as a manually chosen module would.
fn find_pkcs11_module() -> Option<PathBuf> {
    PKCS11_MODULE_CANDIDATES
        .iter()
        .map(Path::new)
        .find(|path| path.exists())
        .map(Path::to_path_buf)
}

/// T-182: tries the typical module paths first; only asks the user to
/// navigate to a `.so` by hand when none of them are present.
fn begin_pkcs11_flow(window: &ApplicationWindow, viewer: &Viewer) {
    match find_pkcs11_module() {
        Some(module_path) => prompt_for_pkcs11_pin(window, viewer, module_path),
        None => show_pkcs11_module_chooser(window, viewer),
    }
}

/// The manual fallback for `begin_pkcs11_flow`: a `GtkFileDialog` filtered to
/// shared-object files, mirroring `show_pfx_chooser`'s shape.
fn show_pkcs11_module_chooser(window: &ApplicationWindow, viewer: &Viewer) {
    let filter = FileFilter::new();
    filter.set_name(Some("PKCS#11 modules"));
    filter.add_pattern("*.so");

    let chooser = FileDialog::builder()
        .title("Choose PKCS#11 module")
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
            prompt_for_pkcs11_pin(&window, &viewer, path);
        }
    });
}

/// T-183: the PIN prompt, and the `Pkcs11CertificateSource::load` call it
/// gates. Same visual pattern as `prompt_for_pfx_password` — a modal
/// `Window` with a `PasswordEntry`, an error label, and Cancel/confirm
/// buttons — with "PIN" rather than "password" in the copy, since that is
/// the term the token itself uses.
///
/// Unlike a `.pfx` password, an incorrect PIN does not fail `load` itself —
/// `Pkcs11CertificateSource` degrades a rejected login to listing only the
/// token's public certificates (see `pin_attempt_is_safe` in
/// `pdf-sign-pkcs11`), so a wrong PIN and an empty token both surface here as
/// zero identities. The empty-case message below covers both and lets the
/// user retry the PIN without re-choosing the module.
fn prompt_for_pkcs11_pin(window: &ApplicationWindow, viewer: &Viewer, module_path: PathBuf) {
    let content = GtkBox::new(GtkOrientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let dialog = Window::builder()
        .transient_for(window)
        .modal(true)
        .title("Card or token PIN required")
        .child(&content)
        .build();

    // Tracked so a later "Use card or token" attempt (which may supersede
    // this one before the background load resolves) can tear this dialog
    // down instead of leaving it stacked underneath a second one — mirrors
    // `prompt_for_pfx_password`/`dismiss_pkcs11_dialog`.
    let stale_dialog = viewer
        .state
        .borrow_mut()
        .pkcs11_dialog
        .replace(dialog.clone());
    if let Some(stale_dialog) = stale_dialog {
        stale_dialog.destroy();
    }

    let pin_entry = PasswordEntry::builder().show_peek_icon(true).build();
    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    let buttons = GtkBox::new(GtkOrientation::Horizontal, 8);
    let cancel = Button::with_label("Cancel");
    let unlock = Button::with_label("Unlock");
    buttons.append(&cancel);
    buttons.append(&unlock);
    content.append(&pin_entry);
    content.append(&error_label);
    content.append(&buttons);
    pin_entry.grab_focus();

    let submit: Rc<dyn Fn()> = Rc::new({
        let window = window.clone();
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        let pin_entry = pin_entry.clone();
        let error_label = error_label.clone();
        let module_path = module_path.clone();
        move || {
            viewer.status.set_text("Reading card or token...");
            dialog.set_sensitive(false);
            glib::spawn_future_local({
                let window = window.clone();
                let viewer = viewer.clone();
                let dialog = dialog.clone();
                let pin_entry = pin_entry.clone();
                let error_label = error_label.clone();
                let module_path = module_path.clone();
                async move {
                    // A newer "Use card or token" attempt may have already
                    // superseded this dialog (torn down above, on entry to a
                    // later `prompt_for_pkcs11_pin` call) while this load was
                    // in flight — same hazard `is_pfx_dialog_current` guards.
                    if !is_pkcs11_dialog_current(&viewer, &dialog) {
                        return;
                    }
                    let pin = pin_entry.text().to_string();
                    match load_pkcs11_in_background(module_path, pin).await {
                        Ok(source) => {
                            let identities = source.list_identities();
                            if identities.is_empty() {
                                dialog.set_sensitive(true);
                                viewer.status.set_text("Waiting for the token PIN.");
                                error_label.set_text(
                                    "No signing identities were found. Check the PIN and \
                                     that the card or token holds a certificate.",
                                );
                                pin_entry.set_text("");
                                pin_entry.grab_focus();
                                return;
                            }
                            viewer
                                .status
                                .set_text(&format_found_identities(&identities));
                            open_identity_picker(&window, &viewer, Arc::new(source), identities);
                            dismiss_pkcs11_dialog(&viewer, &dialog);
                        }
                        Err(Pkcs11AdapterError::Module(message)) => {
                            viewer
                                .status
                                .set_text(&format!("Could not load the PKCS#11 module: {message}"));
                            dismiss_pkcs11_dialog(&viewer, &dialog);
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
    pin_entry.connect_activate(move |_| submit());
    cancel.connect_clicked({
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        move |_| {
            viewer.status.set_text("Card or token selection cancelled.");
            dismiss_pkcs11_dialog(&viewer, &dialog);
        }
    });
    dialog.present();
}

/// Whether `dialog` is still the tracked in-flight PKCS#11 PIN prompt —
/// `false` once a later attempt has superseded it — the PKCS#11 twin of
/// `is_pfx_dialog_current`.
fn is_pkcs11_dialog_current(viewer: &Viewer, dialog: &Window) -> bool {
    viewer.state.borrow().pkcs11_dialog.as_ref() == Some(dialog)
}

/// Clears the tracked PKCS#11 dialog and tears it down — but only if it
/// still points at `dialog`, so this cannot clobber a newer dialog's slot.
/// The PKCS#11 twin of `dismiss_pfx_dialog`.
fn dismiss_pkcs11_dialog(viewer: &Viewer, dialog: &Window) {
    let mut state = viewer.state.borrow_mut();
    if state.pkcs11_dialog.as_ref() == Some(dialog) {
        state.pkcs11_dialog = None;
    }
    drop(state);
    dialog.destroy();
}

async fn load_pkcs11_in_background(
    module_path: PathBuf,
    pin: String,
) -> Result<Pkcs11CertificateSource, Pkcs11AdapterError> {
    gio::spawn_blocking(move || Pkcs11CertificateSource::load(module_path, Some(pin)))
        .await
        .expect("PKCS#11 load task panicked")
}

/// Typical install paths for NSS's software token module across the major
/// Linux packaging layouts — the NSS twin of `PKCS11_MODULE_CANDIDATES`.
/// Tried in order by `find_nss_module` before falling back to a manual file
/// chooser, same reasoning as decision 2 in `docs/batch-digital-signature.md`.
const NSS_MODULE_CANDIDATES: &[&str] = &[
    "/usr/lib/x86_64-linux-gnu/nss/libsoftokn3.so",
    "/usr/lib/i386-linux-gnu/nss/libsoftokn3.so",
    "/usr/lib64/libsoftokn3.so",
    "/usr/lib64/nss/libsoftokn3.so",
    "/usr/lib/libsoftokn3.so",
    "/usr/lib/nss/libsoftokn3.so",
];

/// The NSS twin of `find_pkcs11_module`.
fn find_nss_module() -> Option<PathBuf> {
    NSS_MODULE_CANDIDATES
        .iter()
        .map(Path::new)
        .find(|path| path.exists())
        .map(Path::to_path_buf)
}

/// The shared NSS certificate database most Linux desktops keep certificates
/// in — the database Chrome always reads, and most Firefox profiles are
/// configured to share. `None` when `$HOME` is unset or the directory does
/// not exist, in which case `begin_nss_flow` asks the user to point at their
/// own profile directory instead.
fn default_nss_profile_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let candidate = Path::new(&home).join(".pki").join("nssdb");
    candidate.is_dir().then_some(candidate)
}

/// Entry point for the "Use a certificate from this computer…" button: tries
/// the typical NSS module path and the shared `~/.pki/nssdb` profile first,
/// only asking the user to navigate to either by hand when they are not
/// found — the NSS twin of `begin_pkcs11_flow`.
fn begin_nss_flow(window: &ApplicationWindow, viewer: &Viewer) {
    match find_nss_module() {
        Some(module_path) => continue_nss_flow_with_module(window, viewer, module_path),
        None => show_nss_module_chooser(window, viewer),
    }
}

/// Once the NSS module is known (found automatically or chosen by hand),
/// resolves the profile directory the same way: the shared default first,
/// a manual folder chooser only if that default is not present.
fn continue_nss_flow_with_module(
    window: &ApplicationWindow,
    viewer: &Viewer,
    module_path: PathBuf,
) {
    match default_nss_profile_dir() {
        Some(profile_dir) => prompt_for_nss_password(window, viewer, module_path, profile_dir),
        None => show_nss_profile_chooser(window, viewer, module_path),
    }
}

/// The manual fallback for `begin_nss_flow`'s module step — the NSS twin of
/// `show_pkcs11_module_chooser`.
fn show_nss_module_chooser(window: &ApplicationWindow, viewer: &Viewer) {
    let filter = FileFilter::new();
    filter.set_name(Some("NSS modules"));
    filter.add_pattern("*.so");

    let chooser = FileDialog::builder()
        .title("Choose the NSS certificate module (libsoftokn3.so)")
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
            continue_nss_flow_with_module(&window, &viewer, path);
        }
    });
}

/// The manual fallback for `begin_nss_flow`'s profile-directory step: a
/// folder picker rather than a file picker, since a certificate database is
/// a directory (`cert9.db`/`key4.db`/`pkcs11.txt`), not a single file.
fn show_nss_profile_chooser(window: &ApplicationWindow, viewer: &Viewer, module_path: PathBuf) {
    let chooser = FileDialog::builder()
        .title("Choose the certificate database folder")
        .accept_label("Open")
        .build();
    chooser.select_folder(Some(window), None::<&gio::Cancellable>, {
        let window = window.clone();
        let viewer = viewer.clone();
        move |result| {
            let Ok(folder) = result else {
                return;
            };
            let Some(path) = folder.path() else {
                viewer
                    .status
                    .set_text("The selected location is not a local folder.");
                return;
            };
            prompt_for_nss_password(&window, &viewer, module_path.clone(), path);
        }
    });
}

/// The password prompt for the NSS certificate database, and the
/// `pdf_sign_nss::load` call it gates. Same visual pattern as
/// `prompt_for_pkcs11_pin` — most `~/.pki/nssdb` databases have no password
/// set, so an empty submission is expected to succeed rather than being
/// treated as a mistake.
fn prompt_for_nss_password(
    window: &ApplicationWindow,
    viewer: &Viewer,
    module_path: PathBuf,
    profile_dir: PathBuf,
) {
    let content = GtkBox::new(GtkOrientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let dialog = Window::builder()
        .transient_for(window)
        .modal(true)
        .title("Certificate database password")
        .child(&content)
        .build();

    // Tracked so a later "Use a certificate from this computer" attempt (which
    // may supersede this one before the background load resolves) can tear
    // this dialog down instead of leaving it stacked underneath a second one —
    // mirrors `prompt_for_pkcs11_pin`/`dismiss_nss_dialog`.
    let stale_dialog = viewer.state.borrow_mut().nss_dialog.replace(dialog.clone());
    if let Some(stale_dialog) = stale_dialog {
        stale_dialog.destroy();
    }

    content.append(&Label::new(Some(
        "Leave this blank if your certificates have no password set.",
    )));
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
        let window = window.clone();
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        let password_entry = password_entry.clone();
        let error_label = error_label.clone();
        let module_path = module_path.clone();
        let profile_dir = profile_dir.clone();
        move || {
            viewer.status.set_text("Reading certificate database...");
            dialog.set_sensitive(false);
            glib::spawn_future_local({
                let window = window.clone();
                let viewer = viewer.clone();
                let dialog = dialog.clone();
                let password_entry = password_entry.clone();
                let error_label = error_label.clone();
                let module_path = module_path.clone();
                let profile_dir = profile_dir.clone();
                async move {
                    // A newer attempt may have already superseded this dialog
                    // while this load was in flight — same hazard
                    // `is_pfx_dialog_current`/`is_pkcs11_dialog_current` guard.
                    if !is_nss_dialog_current(&viewer, &dialog) {
                        return;
                    }
                    let password = password_entry.text().to_string();
                    match load_nss_in_background(module_path, profile_dir, password).await {
                        Ok(source) => {
                            let identities = source.list_identities();
                            if identities.is_empty() {
                                dialog.set_sensitive(true);
                                viewer
                                    .status
                                    .set_text("Waiting for the certificate database password.");
                                error_label.set_text(
                                    "No signing certificates were found. Check the password and \
                                     that this computer has certificates imported.",
                                );
                                password_entry.set_text("");
                                password_entry.grab_focus();
                                return;
                            }
                            viewer
                                .status
                                .set_text(&format_found_identities(&identities));
                            open_identity_picker(&window, &viewer, Arc::new(source), identities);
                            dismiss_nss_dialog(&viewer, &dialog);
                        }
                        Err(NssAdapterError::Pkcs11(Pkcs11AdapterError::Module(message))) => {
                            viewer.status.set_text(&format!(
                                "Could not load the certificate database: {message}"
                            ));
                            dismiss_nss_dialog(&viewer, &dialog);
                        }
                        Err(NssAdapterError::InvalidProfilePath(path)) => {
                            viewer.status.set_text(&format!(
                                "The certificate database path {} cannot be used.",
                                path.display()
                            ));
                            dismiss_nss_dialog(&viewer, &dialog);
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
            dismiss_nss_dialog(&viewer, &dialog);
        }
    });
    dialog.present();
}

/// Whether `dialog` is still the tracked in-flight NSS password prompt — the
/// NSS twin of `is_pkcs11_dialog_current`.
fn is_nss_dialog_current(viewer: &Viewer, dialog: &Window) -> bool {
    viewer.state.borrow().nss_dialog.as_ref() == Some(dialog)
}

/// Clears the tracked NSS dialog and tears it down — but only if it still
/// points at `dialog`. The NSS twin of `dismiss_pkcs11_dialog`.
fn dismiss_nss_dialog(viewer: &Viewer, dialog: &Window) {
    let mut state = viewer.state.borrow_mut();
    if state.nss_dialog.as_ref() == Some(dialog) {
        state.nss_dialog = None;
    }
    drop(state);
    dialog.destroy();
}

async fn load_nss_in_background(
    module_path: PathBuf,
    profile_dir: PathBuf,
    password: String,
) -> Result<Pkcs11CertificateSource, NssAdapterError> {
    gio::spawn_blocking(move || pdf_sign_nss::load(module_path, profile_dir, Some(password)))
        .await
        .expect("NSS load task panicked")
}

/// T-184: shows the identities `source` reports and lets the user pick one
/// to sign with. Opened automatically once a `.pfx` password or a PKCS#11
/// PIN unlocks at least one identity — the module doc's "confirming which
/// certificates a file or token unlocks is already useful on its own" was
/// true before this existed; now it is also the entry point into T-185.
fn open_identity_picker(
    window: &ApplicationWindow,
    viewer: &Viewer,
    source: Arc<dyn CertificateSourcePort>,
    identities: Vec<SigningIdentity>,
) {
    let content = GtkBox::new(GtkOrientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let dialog = Window::builder()
        .transient_for(window)
        .modal(true)
        .title("Choose a signing identity")
        .child(&content)
        .build();

    // Tracked so a later successful certificate/token load (which may
    // supersede this one before the user confirms) can tear this dialog
    // down instead of leaving it stacked underneath a second one — mirrors
    // `prompt_for_pfx_password`/`dismiss_pfx_dialog`.
    let stale_dialog = viewer
        .state
        .borrow_mut()
        .sign_picker_dialog
        .replace(dialog.clone());
    if let Some(stale_dialog) = stale_dialog {
        stale_dialog.destroy();
    }

    content.append(&Label::new(Some("Sign this document with:")));

    let (rows, radio_buttons) = build_identity_rows(&identities);
    content.append(&rows);

    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    let buttons = GtkBox::new(GtkOrientation::Horizontal, 8);
    let cancel = Button::with_label("Cancel");
    let sign = Button::with_label("Sign");
    buttons.append(&cancel);
    buttons.append(&sign);
    content.append(&error_label);
    content.append(&buttons);

    sign.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        let identities = identities.clone();
        let radio_buttons = radio_buttons.clone();
        let source = source.clone();
        let error_label = error_label.clone();
        move |_| {
            let Some(chosen) = radio_buttons
                .iter()
                .position(CheckButton::is_active)
                .and_then(|index| identities.get(index))
            else {
                error_label.set_text("Choose a signing identity.");
                return;
            };
            match begin_sign_from_picker(&window, &viewer, source.clone(), chosen.id.clone()) {
                Ok(()) => dismiss_sign_picker(&viewer, &dialog),
                Err(message) => error_label.set_text(message),
            }
        }
    });
    cancel.connect_clicked({
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        move |_| {
            viewer.status.set_text("Signing cancelled.");
            dismiss_sign_picker(&viewer, &dialog);
        }
    });
    dialog.present();
}

/// Builds the picker's identity rows — one grouped `CheckButton` per
/// identity, first pre-selected — separately from the dialog chrome around
/// them, so the row-building itself (labels, grouping, initial selection) is
/// testable without a live dialog. Mirrors `build_sign_content`'s own split,
/// and `forms::fill::build_radio_group`'s grouped-`CheckButton` shape.
fn build_identity_rows(identities: &[SigningIdentity]) -> (GtkBox, Vec<CheckButton>) {
    let rows = GtkBox::new(GtkOrientation::Vertical, 4);
    let radio_buttons: Vec<CheckButton> = identities
        .iter()
        .map(|identity| CheckButton::with_label(&identity.display_name))
        .collect();
    for button in radio_buttons.iter().skip(1) {
        button.set_group(Some(&radio_buttons[0]));
    }
    if let Some(first) = radio_buttons.first() {
        first.set_active(true);
    }
    for button in &radio_buttons {
        rows.append(button);
    }
    (rows, radio_buttons)
}

/// Whether `dialog` is still the tracked identity picker — the signing twin
/// of `is_pfx_dialog_current`.
///
/// `#[cfg(test)]`, unlike its PFX/PKCS#11 siblings: those guard a real race
/// (a background load resolving after a newer prompt superseded it, checked
/// from inside `glib::spawn_future_local`), but `open_identity_picker` opens
/// synchronously and `sign.connect_clicked` dismisses this dialog right in
/// its own handler, never from an async continuation — so there is no
/// production call site to race against, only the supersede test below that
/// asserts the tracking itself.
#[cfg(test)]
fn is_sign_picker_current(viewer: &Viewer, dialog: &Window) -> bool {
    viewer.state.borrow().sign_picker_dialog.as_ref() == Some(dialog)
}

/// Clears the tracked identity picker and tears it down — but only if it
/// still points at `dialog`, so this cannot clobber a newer picker's slot.
/// The signing twin of `dismiss_pfx_dialog`.
fn dismiss_sign_picker(viewer: &Viewer, dialog: &Window) {
    let mut state = viewer.state.borrow_mut();
    if state.sign_picker_dialog.as_ref() == Some(dialog) {
        state.sign_picker_dialog = None;
    }
    drop(state);
    dialog.destroy();
}

/// T-185: the gate and session-state extraction behind the picker's "Sign"
/// button, ending in `document::begin_sign` (the destination chooser,
/// background `pdf_sign::sign_document` call, and save→reopen cycle). `Err`
/// carries the message to show inline in the picker rather than the status
/// bar, so the user can fix the problem — no document, or unsaved changes —
/// without re-choosing a certificate or token.
fn begin_sign_from_picker(
    window: &ApplicationWindow,
    viewer: &Viewer,
    source: Arc<dyn CertificateSourcePort>,
    identity_id: String,
) -> Result<(), &'static str> {
    if let Some(refusal) = signing_refusal(viewer) {
        return Err(refusal);
    }
    let request = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            return Err("Open a PDF before signing.");
        };
        // `sign_document` writes straight from `backing.original_bytes`,
        // bypassing `document_model`/`EditLog` entirely (batch decision 1:
        // `pdf-sign` never depends on `pdf-document`'s editable model).
        // Signing now would silently drop any edit recorded since the last
        // save/reopen — refusing is cheaper and safer than folding those
        // edits in here, and matches this shell's existing care around
        // unsaved work (see the window-close guard in `document.rs`).
        if session.unsaved_to_disk {
            return Err("Save your changes before signing this document.");
        }
        let Some(backing) = session.save_backing.as_ref() else {
            return Err("This document cannot be signed.");
        };
        SignRequest {
            token: SessionToken {
                generation: state.generation,
                edit_revision: session.edit_revision,
            },
            bytes: backing.original_bytes.clone(),
            password: backing.password.clone(),
            page_number: 1,
            field_name: next_signature_field_name(&backing.base),
            source,
            identity_id,
        }
    };
    document::begin_sign(window, viewer, request);
    Ok(())
}

/// The permission gate for signing: a signed field is a new `/FT /Sig` entry
/// in `/AcroForm`/`/Annots`, structurally the same kind of change as placing
/// a form field (batch decision 5). Mirrors
/// `forms::command::structural_edit_refusal` rather than importing it —
/// that function is `pub(super)` to `forms`, so duplicating its one line is
/// cheaper than widening its visibility for this single caller outside it.
fn signing_refusal(viewer: &Viewer) -> Option<&'static str> {
    viewer
        .annotation_editing_refusal()
        .or_else(|| viewer.content_edit_refusal())
}

/// A signature field name unique among any this document's base already
/// carries — `Signature_1`, `Signature_2`, ... — so re-signing an
/// already-signed document (T-179 proves `sign_document` itself allows it)
/// does not collide `/AcroForm /Fields` on a repeated literal name.
fn next_signature_field_name(base: &pdf_manip::LopdfDocument) -> String {
    let existing = base
        .as_lopdf()
        .objects
        .values()
        .filter(|object| {
            object
                .as_dict()
                .ok()
                .and_then(|dict| dict.get(b"FT").ok())
                .and_then(|value| value.as_name().ok())
                == Some(b"Sig".as_slice())
        })
        .count();
    format!("Signature_{}", existing + 1)
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

    /// A `CertificateSourcePort` whose `list_identities` never changes — all
    /// the identity-picker tests below need, since none of them exercise
    /// `sign_digest_raw` (that path is `pdf-sign`'s own, already covered by
    /// `orchestrate.rs`'s tests and `pdf-sign-pfx`/`pdf-sign-pkcs11`'s real
    /// adapters).
    struct StaticCertificateSource(Vec<SigningIdentity>);

    impl CertificateSourcePort for StaticCertificateSource {
        fn list_identities(&self) -> Vec<SigningIdentity> {
            self.0.clone()
        }

        fn sign_digest_raw(
            &self,
            _identity_id: &str,
            _digest: &[u8],
            _algorithm: pdf_sign::SigningAlgorithm,
        ) -> Result<Vec<u8>, pdf_sign::SignError> {
            unimplemented!("not exercised by identity-picker UI tests")
        }
    }

    #[test]
    fn empty_identity_list_reports_the_callers_empty_message() {
        assert_eq!(
            identities_status_message(
                &[],
                "This certificate file has no usable signing identities."
            ),
            "This certificate file has no usable signing identities."
        );
    }

    #[test]
    fn one_identity_uses_the_singular_noun() {
        let message = format_found_identities(&[identity("Alice Doe")]);
        assert_eq!(message, "Found 1 signing identity: Alice Doe");
    }

    #[test]
    fn several_identities_use_the_plural_noun_and_are_all_named() {
        let message = format_found_identities(&[identity("Alice Doe"), identity("Signing Cert 2")]);
        assert_eq!(
            message,
            "Found 2 signing identities: Alice Doe, Signing Cert 2"
        );
    }

    #[test]
    fn no_pkcs11_candidate_exists_on_the_test_host() {
        // The typical Linux module paths are absolute system paths that
        // never exist inside the sandboxed test environment — this pins
        // that assumption so `find_pkcs11_module` falling back to `None`
        // here does not silently start passing for the wrong reason (e.g.
        // an empty candidate list).
        assert!(!PKCS11_MODULE_CANDIDATES.is_empty());
        assert_eq!(find_pkcs11_module(), None);
    }

    /// The NSS twin of the test above.
    #[test]
    fn no_nss_candidate_exists_on_the_test_host() {
        assert!(!NSS_MODULE_CANDIDATES.is_empty());
        assert_eq!(find_nss_module(), None);
    }

    /// `update_sign_controls`'s own gate (`signing_refusal`) reports no
    /// refusal when there is no open document — mirrors
    /// `ui_tests::gtk_ui_starts_with_choose_signing_certificate_enabled`'s
    /// premise, but exercises the function directly rather than relying on
    /// the buttons' untouched construction-time sensitivity.
    #[gtk::test]
    fn gtk_ui_update_sign_controls_leaves_the_certificate_buttons_enabled_with_no_document_open() {
        let application = test_application();
        let built = crate::app::build_ui(&application);

        update_sign_controls(&built.viewer);

        assert!(built.viewer.choose_signing_certificate.is_sensitive());
        assert!(built.viewer.choose_pkcs11_certificate.is_sensitive());
        assert!(built.viewer.choose_nss_certificate.is_sensitive());
        // No document open means `document_is_signed` has nothing to check —
        // the indicator must default to hidden rather than stay however its
        // caller last left it.
        assert!(!built.viewer.signed_indicator.is_visible());

        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_choose_signing_certificate_button_has_the_expected_label() {
        let (button, choose_pkcs11, choose_nss, signed_indicator, content) = build_sign_content();

        assert_eq!(
            button.label().as_deref(),
            Some("Choose signing certificate (.pfx)…")
        );
        assert!(button.is_sensitive());
        assert_eq!(choose_pkcs11.label().as_deref(), Some("Use card or token…"));
        assert!(choose_pkcs11.is_sensitive());
        assert_eq!(
            choose_nss.label().as_deref(),
            Some("Use a certificate from this computer…")
        );
        assert!(choose_nss.is_sensitive());
        assert!(!signed_indicator.is_visible());
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

    /// The PKCS#11 twin of the PFX test above: a second "Use card or token"
    /// attempt must tear down and stop tracking the first PIN prompt.
    #[gtk::test]
    fn gtk_ui_a_second_pkcs11_prompt_supersedes_and_destroys_the_first() {
        let application = test_application();
        let built = crate::app::build_ui(&application);

        prompt_for_pkcs11_pin(&built.window, &built.viewer, PathBuf::from("first.so"));
        let first_dialog = built
            .viewer
            .state
            .borrow()
            .pkcs11_dialog
            .clone()
            .expect("the first prompt must track its dialog");

        prompt_for_pkcs11_pin(&built.window, &built.viewer, PathBuf::from("second.so"));
        let second_dialog = built
            .viewer
            .state
            .borrow()
            .pkcs11_dialog
            .clone()
            .expect("the second prompt must track its own dialog");

        assert!(first_dialog != second_dialog);
        assert!(is_pkcs11_dialog_current(&built.viewer, &second_dialog));
        assert!(!is_pkcs11_dialog_current(&built.viewer, &first_dialog));

        built.window.close();
    }

    /// The NSS twin of the PFX/PKCS#11 tests above: a second "Use a
    /// certificate from this computer" attempt must tear down and stop
    /// tracking the first password prompt.
    #[gtk::test]
    fn gtk_ui_a_second_nss_prompt_supersedes_and_destroys_the_first() {
        let application = test_application();
        let built = crate::app::build_ui(&application);

        prompt_for_nss_password(
            &built.window,
            &built.viewer,
            PathBuf::from("first.so"),
            PathBuf::from("/first/nssdb"),
        );
        let first_dialog = built
            .viewer
            .state
            .borrow()
            .nss_dialog
            .clone()
            .expect("the first prompt must track its dialog");

        prompt_for_nss_password(
            &built.window,
            &built.viewer,
            PathBuf::from("second.so"),
            PathBuf::from("/second/nssdb"),
        );
        let second_dialog = built
            .viewer
            .state
            .borrow()
            .nss_dialog
            .clone()
            .expect("the second prompt must track its own dialog");

        assert!(first_dialog != second_dialog);
        assert!(is_nss_dialog_current(&built.viewer, &second_dialog));
        assert!(!is_nss_dialog_current(&built.viewer, &first_dialog));

        built.window.close();
    }

    #[gtk::test]
    fn gtk_ui_identity_rows_are_labeled_and_the_first_is_preselected() {
        let identities = vec![identity("Alice Doe"), identity("Signing Cert 2")];

        let (_rows, radio_buttons) = build_identity_rows(&identities);

        assert_eq!(radio_buttons.len(), 2);
        assert_eq!(radio_buttons[0].label().as_deref(), Some("Alice Doe"));
        assert_eq!(radio_buttons[1].label().as_deref(), Some("Signing Cert 2"));
        assert!(radio_buttons[0].is_active());
        assert!(!radio_buttons[1].is_active());
    }

    #[gtk::test]
    fn gtk_ui_identity_rows_for_a_single_identity_preselect_it() {
        let identities = vec![identity("Only Signer")];

        let (_rows, radio_buttons) = build_identity_rows(&identities);

        assert_eq!(radio_buttons.len(), 1);
        assert!(radio_buttons[0].is_active());
    }

    /// T-184: opening the picker a second time (a later certificate/token
    /// load resolving while the first picker is still open) must tear down
    /// and stop tracking the first one — the identity-picker twin of the
    /// PFX/PKCS#11 supersede tests above.
    #[gtk::test]
    fn gtk_ui_a_second_identity_picker_supersedes_and_destroys_the_first() {
        let application = test_application();
        let built = crate::app::build_ui(&application);
        let identities = vec![identity("Alice Doe")];
        let source = || -> Arc<dyn CertificateSourcePort> {
            Arc::new(StaticCertificateSource(identities.clone()))
        };

        open_identity_picker(&built.window, &built.viewer, source(), identities.clone());
        let first_dialog = built
            .viewer
            .state
            .borrow()
            .sign_picker_dialog
            .clone()
            .expect("the first picker must track its dialog");

        open_identity_picker(&built.window, &built.viewer, source(), identities);
        let second_dialog = built
            .viewer
            .state
            .borrow()
            .sign_picker_dialog
            .clone()
            .expect("the second picker must track its own dialog");

        assert!(first_dialog != second_dialog);
        assert!(is_sign_picker_current(&built.viewer, &second_dialog));
        assert!(!is_sign_picker_current(&built.viewer, &first_dialog));

        built.window.close();
    }

    /// T-185's gate: signing with no document open is refused before any
    /// destination is asked for, with a message specific enough to act on —
    /// mirrors how `forms::command::command` refuses with `NO_DOCUMENT`.
    #[gtk::test]
    fn gtk_ui_signing_without_an_open_document_is_refused() {
        let application = test_application();
        let built = crate::app::build_ui(&application);
        let identities = vec![identity("Alice Doe")];
        let source: Arc<dyn CertificateSourcePort> = Arc::new(StaticCertificateSource(identities));

        let error =
            begin_sign_from_picker(&built.window, &built.viewer, source, "Alice Doe".to_owned())
                .expect_err("signing with no open document must be refused");

        assert_eq!(error, "Open a PDF before signing.");

        built.window.close();
    }

    #[test]
    fn next_signature_field_name_starts_at_one_for_a_document_with_no_signatures() {
        let base = pdf_manip::create_blank_document(
            pdf_document::PageSize::A4,
            pdf_document::Orientation::Portrait,
        );

        assert_eq!(next_signature_field_name(&base), "Signature_1");
    }

    /// A re-signed document (T-179 proves `sign_document` allows signing
    /// twice) must not repeat a field name already present in the base —
    /// this is what keeps the second signature from colliding with the
    /// first in `/AcroForm /Fields`.
    #[test]
    fn next_signature_field_name_counts_past_an_existing_signature_field() {
        let mut base = pdf_manip::create_blank_document(
            pdf_document::PageSize::A4,
            pdf_document::Orientation::Portrait,
        );
        let mut field = lopdf::Dictionary::new();
        field.set("FT", lopdf::Object::Name(b"Sig".to_vec()));
        base.as_lopdf_mut()
            .add_object(lopdf::Object::Dictionary(field));

        assert_eq!(next_signature_field_name(&base), "Signature_2");
    }
}
