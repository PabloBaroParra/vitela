//! Protect: giving a document password protection it does not already have.
//!
//! The rail's "Protect" item and Home's "Protect" tile both land here, the
//! same way Annotate/Edit/Sign land on their own controls — except this one
//! opens a dialog instead of revealing a panel, because there is no ambient
//! state to reveal: protection is a decision made once, with two passwords,
//! and then written.
//!
//! ## Two passwords, not one
//!
//! Every consumer PDF tool asks for "a password". A PDF has **two** password
//! roles, and they are not interchangeable:
//!
//! - the **open** (user) password, without which the file does not open;
//! - the **permissions** (owner) password, which governs what a reader may do
//!   with the file once open, and which bypasses the restrictions.
//!
//! A single-field dialog has to write one string into both roles, which means
//! everyone who can open the document can also lift its restrictions. That is
//! not a simplification, it is a different security policy — and it is
//! precisely the substitution `pdf_save::RewriteBlocker::IncompleteCredentials`
//! refuses to make on its own when it meets a document that knows only one of
//! its two passwords. Doing by hand what the core refuses to do silently would
//! be a strange thing for this shell to do, so the dialog asks for both and
//! [`validate`] refuses to let them be equal.
//!
//! ## What is *not* decided here
//!
//! The `/P` permission bitmask is written wide open ([`GRANTED_PERMISSIONS`]).
//! Protection here means "this file needs a password", not "this file forbids
//! printing" — a restrictions matrix is its own feature with its own UI, and
//! inventing a default for it would silently impose a policy the user never
//! chose.
//!
//! ## Where the work happens
//!
//! This module owns the dialog and the policy; `document::begin_protect`
//! onwards owns the chooser → confirm → background-save → reopen chain, next
//! to the `save` and `sign` chains it is modelled on.

use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, Button, Label, Orientation as GtkOrientation, PasswordEntry,
    Window,
};
use pdf_document::{
    Credential, EncryptionCredentials, Permissions, SecurityContext, SecurityHandler,
};

use crate::app::document::{self, ProtectRequest};
use crate::app::state::{SessionToken, Viewer};

/// The `/P` bitmask written into a newly protected document: every
/// permission granted. See the module doc — the passwords are the access
/// control, not a restrictions matrix this dialog never asked about.
///
/// The low two bits are reserved by the PDF spec and must be 0, which is what
/// makes this `…FC` rather than `…FF`.
const GRANTED_PERMISSIONS: u32 = 0xFFFF_FFFC;

/// The handler a new protection is written with.
///
/// AES-128 rather than RC4-128: both are implemented by
/// `pdf_save::build_encryption_state`, and RC4 is only still there to
/// *reproduce* the encryption of documents that arrive carrying it. Nothing
/// should create new RC4 documents in 2026.
const NEW_PROTECTION_HANDLER: SecurityHandler = SecurityHandler::Aes128;

/// Why this document cannot be protected, or `None` when it can.
///
/// Two gates, asked in the order the user would hit them:
///
/// - [`Viewer::content_edit_refusal`] — the general "modify the contents of
///   this document" permission bit, the same one the metadata panel reads.
///   Rewriting a file with a new encryption dictionary is squarely a
///   modification, and none of the narrower bits (annotate, extract, print,
///   assembly) describe it.
/// - [`Viewer::full_rewrite_refusal`] — whether this document's *existing*
///   encryption can survive the rewrite that writing new protection requires.
///   Unprotected documents answer `None` here (there is nothing to
///   reproduce), so this only bites when re-protecting a document that was
///   opened with just one of its two passwords — in which case the user is
///   told to reopen with both, which is exactly what changing its passwords
///   needs.
fn protection_refusal(viewer: &Viewer) -> Option<&'static str> {
    viewer
        .content_edit_refusal()
        .or_else(|| viewer.full_rewrite_refusal())
}

/// Opens the Protect dialog, or says why it cannot.
pub(crate) fn begin_protect(viewer: &Viewer) {
    let Some(window) = window_of(viewer) else {
        return;
    };
    if let Some(refusal) = protection_refusal(viewer) {
        viewer.status.set_text(refusal);
        return;
    }
    if viewer.state.borrow().session.is_none() {
        viewer.status.set_text("Open a PDF before protecting it.");
        return;
    }
    prompt_for_passwords(&window, viewer);
}

/// What the two entries have to satisfy before a save is attempted, as a
/// pure function of their text so it can be tested without a dialog.
///
/// Returns the message to show, or `None` when the pair is usable.
fn validate(open_password: &str, permissions_password: &str) -> Option<&'static str> {
    if open_password.is_empty() {
        return Some("Enter the password the document will ask for when it is opened.");
    }
    if permissions_password.is_empty() {
        return Some("Enter the permissions password.");
    }
    if open_password == permissions_password {
        return Some(
            "The two passwords must be different. If they were the same, everyone who can \
             open the document could also change what it permits.",
        );
    }
    None
}

/// The protection the two entries describe.
pub(crate) fn requested_protection(
    open_password: &str,
    permissions_password: &str,
) -> SecurityContext {
    SecurityContext {
        handler: NEW_PROTECTION_HANDLER,
        // `Owner` is what the person applying the protection holds: they
        // chose both passwords, so they are not merely a reader of the
        // result.
        credential: Credential::Owner,
        credentials: EncryptionCredentials::both(open_password, permissions_password),
        permissions: Permissions(GRANTED_PERMISSIONS),
    }
}

/// The dialog: two `PasswordEntry` rows, an error label, Cancel/Protect.
///
/// Modelled on `document::prompt_for_password` and
/// `sign::prompt_for_pfx_password`, with one entry more and no background
/// work of its own — validation is synchronous, and everything after it is
/// `document::begin_protect`'s chooser chain.
fn prompt_for_passwords(window: &ApplicationWindow, viewer: &Viewer) {
    let content = GtkBox::new(GtkOrientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let dialog = Window::builder()
        .transient_for(window)
        .modal(true)
        .title("Protect with a password")
        .child(&content)
        .build();

    let open_label = Label::new(Some("Password to open the document"));
    open_label.set_xalign(0.0);
    let open_entry = PasswordEntry::builder().show_peek_icon(true).build();

    let permissions_label = Label::new(Some("Permissions password"));
    permissions_label.set_xalign(0.0);
    let permissions_entry = PasswordEntry::builder().show_peek_icon(true).build();
    // The one thing a user cannot be expected to know: why a PDF wants two.
    permissions_entry.set_tooltip_text(Some(
        "Governs what readers may do with the document once it is open, and lifts those \
         restrictions for whoever holds it. It must not be the same as the open password.",
    ));

    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    error_label.set_wrap(true);

    let buttons = GtkBox::new(GtkOrientation::Horizontal, 8);
    let cancel = Button::with_label("Cancel");
    let protect = Button::with_label("Protect");
    buttons.append(&cancel);
    buttons.append(&protect);

    content.append(&open_label);
    content.append(&open_entry);
    content.append(&permissions_label);
    content.append(&permissions_entry);
    content.append(&error_label);
    content.append(&buttons);
    open_entry.grab_focus();

    let submit = {
        let window = window.clone();
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        let open_entry = open_entry.clone();
        let permissions_entry = permissions_entry.clone();
        let error_label = error_label.clone();
        move || {
            let open_password = open_entry.text().to_string();
            let permissions_password = permissions_entry.text().to_string();
            if let Some(message) = validate(&open_password, &permissions_password) {
                error_label.set_text(message);
                return;
            }
            match build_request(&viewer, &open_password, &permissions_password) {
                Some(request) => {
                    dialog.destroy();
                    document::begin_protect(&window, &viewer, request);
                }
                None => error_label.set_text("This document cannot be protected."),
            }
        }
    };

    protect.connect_clicked({
        let submit = submit.clone();
        move |_| submit()
    });
    // Enter in the first entry moves on rather than submitting a half-filled
    // form; Enter in the second submits, the same as every other prompt in
    // this shell.
    open_entry.connect_activate({
        let permissions_entry = permissions_entry.clone();
        move |_| {
            permissions_entry.grab_focus();
        }
    });
    permissions_entry.connect_activate({
        let submit = submit.clone();
        move |_| submit()
    });
    cancel.connect_clicked({
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        move |_| {
            viewer.status.set_text("Protection cancelled.");
            dialog.destroy();
        }
    });

    dialog.present();
}

/// Bundles what the save needs out of the live session.
///
/// `None` when the session has no editable model or no save backing — the
/// same two things an ordinary save needs, checked here so the dialog can say
/// so before a chooser opens rather than after one is dismissed.
fn build_request(
    viewer: &Viewer,
    open_password: &str,
    permissions_password: &str,
) -> Option<ProtectRequest> {
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    Some(ProtectRequest {
        token: SessionToken {
            generation: state.generation,
            edit_revision: session.edit_revision,
        },
        document: session.document_model.clone()?,
        backing: session.save_backing.clone()?,
        sources: session.imported_sources.clone(),
        protection: requested_protection(open_password, permissions_password),
    })
}

/// See `content_edit::window_of` — a rail click carries no window parameter
/// of its own, and `home::tools::apply` dispatches every tool without one.
fn window_of(viewer: &Viewer) -> Option<ApplicationWindow> {
    viewer
        .status
        .root()
        .and_then(|root| root.downcast::<ApplicationWindow>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ui_tests::built_ui;

    /// The gate a cold start hits. The rail item and the Home tile are both
    /// live with no document open -- they route through the file chooser
    /// first -- so reaching this function without a session has to say so,
    /// rather than open a dialog asking for two passwords to protect nothing.
    ///
    /// It also pins the window recovery: `begin_protect` returns silently
    /// when `window_of` finds no `ApplicationWindow` to parent a modal
    /// against, so a status message here is proof that lookup worked.
    #[gtk::test]
    fn gtk_ui_protecting_without_an_open_document_is_refused() {
        let built = built_ui();

        begin_protect(&built.viewer);

        assert_eq!(
            built.viewer.status.text().as_str(),
            "Open a PDF before protecting it."
        );

        built.window.close();
    }

    #[test]
    fn an_empty_open_password_is_refused() {
        assert!(validate("", "perms").is_some());
    }

    #[test]
    fn an_empty_permissions_password_is_refused() {
        assert!(validate("open", "").is_some());
    }

    /// The decision this dialog exists to enforce: one password in both roles
    /// is a different security policy, not a shorter form.
    #[test]
    fn the_same_password_in_both_roles_is_refused() {
        assert!(validate("hunter2", "hunter2").is_some());
    }

    #[test]
    fn two_different_passwords_are_accepted() {
        assert_eq!(validate("open-pw", "perms-pw"), None);
    }

    /// The context handed to `pdf-save` must carry *both* roles, or
    /// `build_encryption_state` refuses it as `IncompleteCredentials` — the
    /// refusal would arrive as an error string long after the dialog closed.
    #[test]
    fn the_requested_protection_carries_both_password_roles() {
        let protection = requested_protection("open-pw", "perms-pw");

        assert_eq!(
            protection.credentials.complete(),
            Some(("open-pw", "perms-pw"))
        );
        assert_eq!(protection.handler, SecurityHandler::Aes128);
        assert_eq!(protection.permissions, Permissions(GRANTED_PERMISSIONS));
    }
}
