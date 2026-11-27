//! The toast: one card floating over the window's content, showing the
//! outcome the user most recently caused.
//!
//! Widgets and one timer only. Which toast shows, which wait and for how
//! long is `queue`'s; what counts as an outcome is `feedback`'s.
//!
//! * **No focus.** Nothing here is focusable and nothing grabs the focus, so
//!   a toast arriving mid-keystroke changes nothing about where typing goes.
//!   The keyboard's way to dismiss one is Escape (`escape`).
//! * **No blocked clicks.** The host is an overlay child aligned to the
//!   bottom centre, so it is allocated its own size and nothing more; a
//!   click anywhere else lands on the content under it. While nothing shows
//!   it is not visible at all.
//! * **Announced.** Each card is built with the role of what it reports — an
//!   alert for an error or a refusal, a status for a success — so assistive
//!   technology reads it when it appears without the focus moving to it.
//! * **Animated only if the desktop animates.** The fade is a `GtkRevealer`,
//!   which already skips its transition when `gtk-enable-animations` is off.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use gtk::prelude::*;
use gtk::{glib, AccessibleRole, Align, Box as GtkBox, Button, Label, Orientation, Revealer};

use super::queue::{self, Pushed, Queue, Toast, Tone};
use crate::app::i18n::tr;

/// The card's look, from the shared palette (`theme`) so it follows light
/// and dark. The tone is the stripe down the leading edge.
pub(crate) const TOAST_CSS: &str = r#"
.toast {
  margin: 12px;
  padding: 8px 8px 8px 12px;
  background: @vitela_surface;
  color: @vitela_text;
  border: 1px solid @vitela_border_strong;
  border-left-width: 4px;
  border-radius: 8px;
  box-shadow: 0 4px 16px alpha(@vitela_border_strong, 0.6);
}

.toast-success {
  border-left-color: @vitela_success;
}

.toast-refusal {
  border-left-color: @vitela_warning;
}

.toast-error {
  border-left-color: @vitela_danger;
}

.toast .toast-close {
  min-width: 22px;
  min-height: 22px;
  padding: 0 4px;
  border: none;
  border-radius: 6px;
  background: transparent;
  box-shadow: none;
  color: @vitela_text_muted;
}

.toast .toast-close:hover {
  background: @vitela_hover;
  color: @vitela_text;
}
"#;

/// The close button's face: a symbol, not an icon-theme lookup (see
/// `shell`). Its accessible name says it in words.
const CLOSE_MARKER: &str = "\u{00d7}";
/// How long the fade takes when the desktop animates.
const FADE_MS: u32 = 150;
/// Clear of the status bar, which sits under the content the toast floats
/// over.
const BOTTOM_MARGIN: i32 = 36;

fn tone_class(tone: Tone) -> &'static str {
    match tone {
        Tone::Success => "toast-success",
        Tone::Refusal => "toast-refusal",
        Tone::Error => "toast-error",
    }
}

/// An error or a refusal interrupts; a success is only reported.
fn role(tone: Tone) -> AccessibleRole {
    match tone {
        Tone::Success => AccessibleRole::Status,
        Tone::Refusal | Tone::Error => AccessibleRole::Alert,
    }
}

/// The window's toast area. Cheap to clone: every clone is the same host.
#[derive(Clone)]
pub(crate) struct ToastHost {
    inner: Rc<Inner>,
}

struct Inner {
    revealer: Revealer,
    queue: RefCell<Queue>,
    /// The timer that takes the toast on screen away, while one is running.
    timer: RefCell<Option<glib::SourceId>>,
    /// How long a toast stays. Production's is `queue::lifetime`; a window
    /// built by a test starts with none, so no case leaves a timer behind
    /// for the next one.
    lifetime: Cell<fn(Tone, &str) -> Option<Duration>>,
    /// Told whenever the toast on screen changes.
    changed: RefCell<Option<Rc<dyn Fn()>>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
    }
}

fn default_lifetime() -> fn(Tone, &str) -> Option<Duration> {
    if cfg!(test) {
        |_, _| None
    } else {
        queue::lifetime
    }
}

impl ToastHost {
    pub(crate) fn new() -> Self {
        let revealer = Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .transition_duration(FADE_MS)
            .halign(Align::Center)
            .valign(Align::End)
            .margin_bottom(BOTTOM_MARGIN)
            .visible(false)
            .can_focus(false)
            .build();
        // Out of the way entirely once the fade has run: a hidden revealer
        // still holds its child's size, and would swallow clicks meant for
        // what is under it.
        revealer.connect_child_revealed_notify(|revealer| {
            if !revealer.reveals_child() && !revealer.is_child_revealed() {
                revealer.set_visible(false);
                revealer.set_child(gtk::Widget::NONE);
            }
        });
        Self {
            inner: Rc::new(Inner {
                revealer,
                queue: RefCell::default(),
                timer: RefCell::default(),
                lifetime: Cell::new(default_lifetime()),
                changed: RefCell::default(),
            }),
        }
    }

    /// What goes on the window's root overlay.
    pub(crate) fn widget(&self) -> &Revealer {
        &self.inner.revealer
    }

    /// Runs `listener` whenever the toast on screen changes — one appeared,
    /// was replaced, or the last one left.
    pub(crate) fn connect_changed(&self, listener: impl Fn() + 'static) {
        *self.inner.changed.borrow_mut() = Some(Rc::new(listener));
    }

    /// Shows `text` now, in `tone`. An empty message shows nothing.
    pub(crate) fn show(&self, tone: Tone, text: &str) {
        if text.is_empty() {
            return;
        }
        let pushed = self.inner.queue.borrow_mut().push(Toast {
            tone,
            text: text.to_owned(),
        });
        match pushed {
            Pushed::Shown => self.present(),
            Pushed::Restarted => self.restart_timer(),
        }
    }

    /// Takes the toast on screen away and shows the one waiting behind it,
    /// if any. Does nothing with no toast up.
    pub(crate) fn dismiss(&self) {
        if self.inner.queue.borrow().current().is_none() {
            return;
        }
        self.inner.queue.borrow_mut().dismiss();
        self.present();
    }

    /// The toast on screen.
    pub(crate) fn current(&self) -> Option<Toast> {
        self.inner.queue.borrow().current().cloned()
    }

    /// Whether a toast is on screen.
    pub(crate) fn is_showing(&self) -> bool {
        self.inner.queue.borrow().current().is_some()
    }

    #[cfg(test)]
    pub(crate) fn set_lifetime(&self, lifetime: fn(Tone, &str) -> Option<Duration>) {
        self.inner.lifetime.set(lifetime);
    }

    /// Puts the queue's current toast on screen, or clears the screen when
    /// it has none.
    fn present(&self) {
        let current = self.current();
        let revealer = &self.inner.revealer;
        match &current {
            Some(toast) => {
                revealer.set_child(Some(&self.card(toast)));
                revealer.set_visible(true);
                revealer.set_reveal_child(true);
            }
            None => revealer.set_reveal_child(false),
        }
        self.restart_timer();
        // Cloned out first: the listener may show or dismiss a toast.
        let changed = self.inner.changed.borrow().clone();
        if let Some(changed) = changed {
            changed();
        }
    }

    fn card(&self, toast: &Toast) -> GtkBox {
        let card = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .accessible_role(role(toast.tone))
            .can_focus(false)
            .build();
        card.add_css_class("toast");
        card.add_css_class(tone_class(toast.tone));

        let label = Label::new(Some(&toast.text));
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        label.set_max_width_chars(60);
        card.append(&label);

        if queue::has_close_button(toast.tone) {
            let close = Button::with_label(CLOSE_MARKER);
            close.add_css_class("toast-close");
            close.set_valign(Align::Start);
            // Never the focus widget: when the card goes, GTK would have to
            // move the focus somewhere of its own choosing.
            close.set_focusable(false);
            close.set_focus_on_click(false);
            let dismiss = tr("toast-dismiss");
            close.set_tooltip_text(Some(&dismiss));
            close.update_property(&[gtk::accessible::Property::Label(&dismiss)]);
            close.connect_clicked({
                let host = self.downgrade();
                move |_| {
                    if let Some(host) = host.upgrade() {
                        ToastHost { inner: host }.dismiss();
                    }
                }
            });
            card.append(&close);
        }
        card
    }

    fn downgrade(&self) -> Weak<Inner> {
        Rc::downgrade(&self.inner)
    }

    /// Starts the current toast's time over, or stops the clock when it has
    /// none: nothing is up, or what is up stays until dismissed.
    fn restart_timer(&self) {
        if let Some(timer) = self.inner.timer.borrow_mut().take() {
            timer.remove();
        }
        let lifetime = self
            .current()
            .and_then(|toast| (self.inner.lifetime.get())(toast.tone, &toast.text));
        let Some(lifetime) = lifetime else {
            return;
        };
        let host = self.downgrade();
        let timer = glib::timeout_add_local_once(lifetime, move || {
            let Some(inner) = host.upgrade() else {
                return;
            };
            // The source is gone once it has run: forget its id before
            // anything can try to remove it.
            inner.timer.borrow_mut().take();
            ToastHost { inner }.dismiss();
        });
        *self.inner.timer.borrow_mut() = Some(timer);
    }
}
