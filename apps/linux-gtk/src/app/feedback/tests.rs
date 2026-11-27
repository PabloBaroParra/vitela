//! The feedback channels over a real window: which messages reach the toast,
//! what survives the status line being overwritten, and how the toast sits
//! in the window.

use std::time::Duration;

use gtk::prelude::*;
use gtk::AccessibleRole;

use super::queue::Toast;
use super::*;
use crate::app::i18n::tr;
use crate::app::test_support::{pump, pump_for, wait_until};
use crate::app::ui_tests::built_ui;
use crate::app::BuiltUi;

/// A window that closes when the case ends, also on a failed assertion:
/// `#[gtk::test]` cases share one thread.
struct Window(BuiltUi);

impl Window {
    fn new() -> Self {
        Self(built_ui())
    }

    fn shown() -> Self {
        let window = Self::new();
        window.0.window.present();
        pump();
        window
    }

    fn viewer(&self) -> &Viewer {
        &self.0.viewer
    }

    fn toasts(&self) -> &ToastHost {
        &self.viewer().feedback.toasts
    }

    fn toast(&self) -> Option<(Tone, String)> {
        self.toasts()
            .current()
            .map(|toast| (toast.tone, toast.text))
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.0.viewer.state.borrow_mut().session = None;
        self.0.window.close();
    }
}

fn error(text: &str) -> Option<(Tone, String)> {
    Some((Tone::Error, text.to_owned()))
}

#[test]
fn ambient_messages_stay_on_the_status_line_and_outcomes_go_to_a_toast() {
    assert_eq!(channel(Notice::Info), Channel::StatusLine);
    assert_eq!(channel(Notice::Success), Channel::Toast(Tone::Success));
    assert_eq!(channel(Notice::Refusal), Channel::Toast(Tone::Refusal));
    assert_eq!(channel(Notice::Error), Channel::Toast(Tone::Error));
}

/// The defect this module exists for: the viewport reporting the visible
/// pages used to overwrite whatever error the status line held.
#[gtk::test]
fn gtk_ui_a_scroll_report_does_not_erase_an_error() {
    let window = Window::new();
    let viewer = window.viewer();

    viewer.fail("Could not open PDF: not a PDF.");
    viewer.info("Showing pages 3-4 of 12.");
    viewer.info("Showing pages 4-5 of 12.");

    assert_eq!(window.toast(), error("Could not open PDF: not a PDF."));
    assert_eq!(viewer.status.text(), "Showing pages 4-5 of 12.");
}

#[gtk::test]
fn gtk_ui_ambient_information_never_raises_a_toast() {
    let window = Window::new();

    window.viewer().info("Showing pages 1-2 of 12.");

    assert_eq!(window.toast(), None);
    assert!(!window.toasts().widget().is_visible());
}

/// "Saving…" is not true any more once the save has an outcome.
#[gtk::test]
fn gtk_ui_an_outcome_clears_the_progress_the_status_line_was_showing() {
    let window = Window::new();
    let viewer = window.viewer();
    viewer.info("Saving…");

    viewer.success("Saved report.pdf.");

    assert_eq!(viewer.status.text(), "");
    assert_eq!(
        window.toast(),
        Some((Tone::Success, "Saved report.pdf.".to_owned()))
    );
}

/// A real refusal, through the code that makes it: Export with no document.
#[gtk::test]
fn gtk_ui_a_refused_export_is_reported_on_a_toast() {
    let window = Window::shown();

    crate::app::export::begin_export(window.viewer());

    assert_eq!(
        window.toast(),
        Some((Tone::Refusal, tr("export-no-document")))
    );
}

#[gtk::test]
fn gtk_ui_a_newer_outcome_replaces_the_toast_and_a_displaced_error_returns() {
    let window = Window::new();
    let viewer = window.viewer();

    viewer.fail("Could not save.");
    viewer.success("Exported 3 images.");
    assert_eq!(
        window.toast(),
        Some((Tone::Success, "Exported 3 images.".to_owned()))
    );

    window.toasts().dismiss();
    assert_eq!(window.toast(), error("Could not save."));

    window.toasts().dismiss();
    assert_eq!(window.toast(), None);
}

fn quick(tone: Tone, _text: &str) -> Option<Duration> {
    (tone != Tone::Error).then_some(Duration::from_millis(20))
}

/// The timer is switched on for this case alone and has run out before it
/// ends, so nothing is left ticking for the next one.
#[gtk::test]
fn gtk_ui_a_success_leaves_by_itself_and_an_error_waits_to_be_dismissed() {
    let window = Window::shown();
    window.toasts().set_lifetime(quick);

    window.viewer().success("Saved.");
    assert!(window.toasts().is_showing());
    assert!(
        wait_until(|| !window.toasts().is_showing()),
        "a success times out"
    );

    window.viewer().fail("Could not save.");
    pump_for(Duration::from_millis(80));
    assert_eq!(window.toast(), error("Could not save."));

    close_button(window.toasts())
        .expect("an error has a close button")
        .emit_clicked();
    assert_eq!(window.toast(), None);
    assert!(wait_until(|| !window.toasts().widget().is_visible()));
}

/// Saying the same thing again must not stack a second copy behind the
/// first, or dismissing it would bring it straight back.
#[gtk::test]
fn gtk_ui_the_same_error_twice_is_one_toast() {
    let window = Window::new();

    window.viewer().fail("Could not save.");
    window.viewer().fail("Could not save.");
    window.toasts().dismiss();

    assert_eq!(window.toast(), None);
}

fn card(host: &ToastHost) -> gtk::Widget {
    host.widget().child().expect("a toast is up")
}

fn close_button(host: &ToastHost) -> Option<gtk::Button> {
    let mut child = card(host).first_child();
    while let Some(widget) = child {
        if let Ok(button) = widget.clone().downcast::<gtk::Button>() {
            return Some(button);
        }
        child = widget.next_sibling();
    }
    None
}

#[gtk::test]
fn gtk_ui_only_a_toast_that_lingers_has_a_close_button() {
    let window = Window::new();

    window.viewer().success("Saved.");
    assert!(close_button(window.toasts()).is_none());

    window.viewer().refuse("Wait for the save to finish.");
    assert!(close_button(window.toasts()).is_some());

    window.viewer().fail("Could not save.");
    let close = close_button(window.toasts()).expect("an error has one");
    assert_eq!(
        close.tooltip_text().as_deref(),
        Some(tr("toast-dismiss").as_str())
    );
}

/// What assistive technology hears: an alert for what interrupts, a status
/// for what merely reports.
#[gtk::test]
fn gtk_ui_a_toast_is_announced_by_the_role_of_what_it_reports() {
    let window = Window::new();

    window.viewer().success("Saved.");
    assert_eq!(
        card(window.toasts()).accessible_role(),
        AccessibleRole::Status
    );

    window.viewer().refuse("Wait for the save to finish.");
    assert_eq!(
        card(window.toasts()).accessible_role(),
        AccessibleRole::Alert
    );

    window.viewer().fail("Could not save.");
    assert_eq!(
        card(window.toasts()).accessible_role(),
        AccessibleRole::Alert
    );
}

#[gtk::test]
fn gtk_ui_a_toast_carries_its_tone_as_a_style_class() {
    let window = Window::new();
    for (say, class) in [
        (Viewer::success as fn(&Viewer, &str), "toast-success"),
        (Viewer::refuse, "toast-refusal"),
        (Viewer::fail, "toast-error"),
    ] {
        say(window.viewer(), class);
        let card = card(window.toasts());
        assert!(card.has_css_class("toast"));
        assert!(card.has_css_class(class), "{class}");
    }
}

/// A toast arriving while the user types must not move the focus, and must
/// not offer anything the focus could later be stranded on.
#[gtk::test]
fn gtk_ui_a_toast_takes_no_keyboard_focus() {
    let window = Window::shown();
    let viewer = window.viewer();
    let field = gtk::Entry::new();
    viewer.pages.append(&field);
    viewer
        .view_stack
        .set_visible_child_name(crate::app::home::EDITOR_PAGE);
    pump();
    field.grab_focus();
    let focused = gtk::prelude::GtkWindowExt::focus(&window.0.window);
    assert!(focused.is_some(), "the field took the focus");

    viewer.fail("Could not save.");
    pump();

    assert_eq!(gtk::prelude::GtkWindowExt::focus(&window.0.window), focused);
    assert!(!window.toasts().widget().can_focus());
    let close = close_button(window.toasts()).expect("an error has a close button");
    assert!(!close.is_focusable());
}

/// The host is allocated the card and nothing more, so a press anywhere
/// else lands on the content under it.
#[gtk::test]
fn gtk_ui_a_toast_leaves_the_rest_of_the_window_to_the_pointer() {
    let window = Window::shown();
    let host = window.toasts().widget().clone();
    assert_eq!(host.halign(), gtk::Align::Center);
    assert_eq!(host.valign(), gtk::Align::End);

    window.viewer().fail("Could not save.");
    pump();

    let root = window.0.window.child().expect("the window has content");
    let inside_toast = |x: f64, y: f64| {
        root.pick(x, y, gtk::PickFlags::DEFAULT)
            .is_some_and(|widget| widget == host || widget.is_ancestor(&host))
    };
    assert!(!inside_toast(4.0, 4.0), "the top-left corner is content");
    assert!(
        !inside_toast(
            f64::from(root.width()) - 4.0,
            f64::from(root.height()) / 2.0
        ),
        "so is the right edge"
    );
    let bounds = host.compute_bounds(&root).expect("the toast is allocated");
    assert!(
        inside_toast(
            f64::from(bounds.x() + bounds.width() / 2.0),
            f64::from(bounds.y() + bounds.height() / 2.0),
        ),
        "the toast itself takes its own clicks"
    );
    assert!(
        bounds.width() < root.width() as f32,
        "it is not a full-width bar"
    );
}

/// With nothing to show the host is out of the picture altogether.
#[gtk::test]
fn gtk_ui_a_dismissed_toast_leaves_nothing_in_the_way() {
    let window = Window::shown();

    window.viewer().fail("Could not save.");
    pump();
    assert!(window.toasts().widget().is_visible());

    window.toasts().dismiss();

    assert!(wait_until(|| !window.toasts().widget().is_visible()));
    assert!(window.toasts().widget().child().is_none());
}

/// Restores `gtk-enable-animations` on the way out, also on an unwind: the
/// setting is global and every later case would inherit it.
struct Animations(bool);

impl Animations {
    fn set(enabled: bool) -> Self {
        let settings = gtk::Settings::default().expect("gtk settings");
        let previous = settings.is_gtk_enable_animations();
        settings.set_gtk_enable_animations(enabled);
        Self(previous)
    }
}

impl Drop for Animations {
    fn drop(&mut self) {
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_enable_animations(self.0);
        }
    }
}

/// The fade is the revealer's, and the revealer skips it when the desktop
/// has animations off: the toast is fully there, and fully gone, at once.
#[gtk::test]
fn gtk_ui_a_toast_does_not_fade_when_animations_are_off() {
    let _animations = Animations::set(false);
    let window = Window::shown();
    let host = window.toasts().widget().clone();

    window.viewer().fail("Could not save.");
    assert!(host.is_child_revealed(), "shown without a transition");

    window.toasts().dismiss();
    assert!(!host.is_child_revealed(), "gone without one");
    assert!(!host.is_visible());
}

#[gtk::test]
fn gtk_ui_an_empty_outcome_shows_nothing() {
    let window = Window::new();

    window.viewer().fail("");

    assert_eq!(window.toasts().current(), None::<Toast>);
}

/// What the tests read back: the last thing said on either channel.
#[gtk::test]
fn gtk_ui_the_last_thing_said_follows_whichever_channel_spoke_last() {
    let window = Window::new();
    let viewer = window.viewer();
    let log = viewer.said_log();

    viewer.info("Opening…");
    assert_eq!(viewer.last_said(), "Opening…");
    viewer.fail("Could not open.");
    assert_eq!(viewer.last_said(), "Could not open.");
    viewer.info("Showing pages 1-2 of 3.");
    assert_eq!(viewer.last_said(), "Showing pages 1-2 of 3.");
    // Repeating what is already showing is not news.
    viewer.info("Showing pages 1-2 of 3.");

    assert_eq!(
        *log.borrow(),
        ["Opening…", "Could not open.", "Showing pages 1-2 of 3."]
    );
}
