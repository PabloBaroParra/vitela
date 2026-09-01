use glib::translate::{from_glib_full, ToGlibPtr};
use gtk::prelude::*;
use gtk::{gio, glib, Application};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{build_ui, ensure_built, BuiltUi, APPLICATION_ID};

static TEST_APPLICATION_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

fn test_application() -> Application {
    let sequence = TEST_APPLICATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let application = Application::builder()
        .application_id(format!("{APPLICATION_ID}.test.case{sequence}"))
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application
        .register(None::<&gio::Cancellable>)
        .expect("the GTK test application must register");
    application
}

fn drain_main_context() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

#[gtk::test]
fn gtk_ui_exposes_the_exact_search_accessibility_label_without_presenting() {
    let application = test_application();
    let built = build_ui(&application);

    assert!(!built.window.is_visible());

    let accessible: &gtk::Accessible = built.viewer.search_entry.as_ref();
    let expected = <str as ToGlibPtr<'_, *const std::ffi::c_char>>::to_glib_none("Search document");
    let mismatch: Option<glib::GString> = unsafe {
        from_glib_full(gtk::ffi::gtk_test_accessible_check_property(
            accessible.to_glib_none().0,
            gtk::ffi::GTK_ACCESSIBLE_PROPERTY_LABEL,
            expected.0,
        ))
    };
    assert!(
        mismatch.is_none(),
        "search accessible label mismatch: {mismatch:?}"
    );

    built.window.close();
    drain_main_context();
}

#[gtk::test]
fn gtk_ui_starts_with_find_navigation_disabled() {
    let application = test_application();
    let built = build_ui(&application);

    assert!(!built.viewer.find_previous.is_sensitive());
    assert!(!built.viewer.find_next.is_sensitive());

    built.window.close();
    drain_main_context();
}

#[gtk::test]
fn gtk_ui_builds_an_accessible_three_column_editor_shell() {
    let application = test_application();
    let built = build_ui(&application);

    assert!(built
        .viewer
        .page_navigation
        .has_css_class("page-navigation"));

    let accessible: &gtk::Accessible = built.viewer.page_navigation.as_ref();
    let expected = <str as ToGlibPtr<'_, *const std::ffi::c_char>>::to_glib_none("Pages");
    let mismatch: Option<glib::GString> = unsafe {
        from_glib_full(gtk::ffi::gtk_test_accessible_check_property(
            accessible.to_glib_none().0,
            gtk::ffi::GTK_ACCESSIBLE_PROPERTY_LABEL,
            expected.0,
        ))
    };
    assert!(
        mismatch.is_none(),
        "page navigation accessible label mismatch: {mismatch:?}"
    );

    built.window.close();
    drain_main_context();
}

#[gtk::test]
fn gtk_ui_starts_with_no_document_open_page_and_zoom_readouts() {
    let application = test_application();
    let built = build_ui(&application);

    assert_eq!(built.viewer.page_indicator.text(), "\u{2013}");
    assert_eq!(built.viewer.zoom_label.text(), "100%");

    built.window.close();
    drain_main_context();
}

#[gtk::test]
fn gtk_ui_starts_with_document_output_controls_disabled() {
    let application = test_application();
    let built = build_ui(&application);

    assert!(!built.viewer.print_button.is_sensitive());
    assert!(!built.viewer.save_button.is_sensitive());

    built.window.close();
    drain_main_context();
}

#[gtk::test]
fn gtk_ui_starts_with_undo_action_present_and_disabled() {
    let application = test_application();
    let built = build_ui(&application);

    assert!(!built.viewer.undo_action.is_enabled());
    assert!(built.window.lookup_action("undo").is_some());

    built.window.close();
    drain_main_context();
}

#[gtk::test]
fn gtk_ui_starts_with_redo_action_present_and_disabled() {
    let application = test_application();
    let built = build_ui(&application);

    assert!(!built.viewer.redo_action.is_enabled());
    assert!(built.window.lookup_action("redo").is_some());

    built.window.close();
    drain_main_context();
}

#[gtk::test]
fn gtk_ui_allows_a_clean_window_to_close_without_a_prompt() {
    let application = test_application();
    let built = build_ui(&application);
    built.window.present();
    drain_main_context();

    built.window.close();
    drain_main_context();

    assert!(!built.window.is_visible());
}

/// Batch B23 Fase 2/3: choosing a signing certificate never depends on a
/// document being open (you might unlock your `.pfx` or token before you
/// even pick a file to sign), unlike `print_button`/`save_button` above — so
/// these buttons start enabled rather than disabled.
#[gtk::test]
fn gtk_ui_starts_with_choose_signing_certificate_enabled() {
    let application = test_application();
    let built = build_ui(&application);

    assert!(built.viewer.choose_signing_certificate.is_sensitive());
    assert!(built.viewer.choose_pkcs11_certificate.is_sensitive());

    built.window.close();
    drain_main_context();
}

/// `run`'s `activate`/`open` handlers both go through `ensure_built` so a
/// second file-manager launch (`open`) lands in the window the first launch
/// (`activate`) already made, instead of spawning a second one.
#[gtk::test]
fn gtk_ui_ensure_built_reuses_the_same_window_on_repeated_calls() {
    let application = test_application();
    let built_ui: Rc<RefCell<Option<BuiltUi>>> = Rc::new(RefCell::new(None));

    let first = ensure_built(&built_ui, &application);
    let second = ensure_built(&built_ui, &application);

    assert_eq!(first.window, second.window);

    first.window.close();
    drain_main_context();
}
