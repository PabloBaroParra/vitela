//! Where the shell says what happened: one classified entry point, two
//! places a message can go.
//!
//! The status line used to be the only channel. It is one line that the
//! next thing to speak overwrites, and the most talkative speaker is the
//! viewport — every scroll that changes the visible pages says so. An open,
//! save or export failure could be gone before it was read.
//!
//! So a message now says what it *is*, and that decides where it goes:
//!
//! * [`Notice::Info`] — ambient: the visible page range, a tool being armed,
//!   a mode's hint, work in progress, a routine edit being recorded. Stays
//!   on the status line, where the next one may replace it.
//! * [`Notice::Success`], [`Notice::Refusal`], [`Notice::Error`] — an
//!   outcome: something the user asked for finished, was declined, or
//!   failed. Goes to a toast (`toast`), which no scroll can touch.
//!
//! An outcome also clears the status line: whatever it said — "Saving…",
//! most often — is no longer true once the outcome is in.
//!
//! Call sites go through [`Viewer::info`], [`Viewer::success`],
//! [`Viewer::refuse`] and [`Viewer::fail`]; none of them touches the label
//! or the toast itself.

mod queue;
mod toast;

#[cfg(test)]
mod tests;

pub(crate) use queue::Tone;
pub(crate) use toast::{ToastHost, TOAST_CSS};

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib;

use super::state::Viewer;

/// What a message is. See the module documentation for which is which.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Notice {
    /// Ambient information; nothing was asked for or went wrong.
    Info,
    /// Something the user asked for is done.
    Success,
    /// The shell declined to do something. Nothing was changed or lost.
    Refusal,
    /// An operation was attempted and failed.
    Error,
}

/// Where a message is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Channel {
    StatusLine,
    Toast(Tone),
}

/// Where a `notice` goes.
pub(crate) fn channel(notice: Notice) -> Channel {
    match notice {
        Notice::Info => Channel::StatusLine,
        Notice::Success => Channel::Toast(Tone::Success),
        Notice::Refusal => Channel::Toast(Tone::Refusal),
        Notice::Error => Channel::Toast(Tone::Error),
    }
}

/// A running list of messages, shared with the test that asked for it.
#[cfg(test)]
pub(crate) type SaidLog = Rc<RefCell<Vec<String>>>;

/// The window's feedback beyond the status line itself: the toast area, and
/// what was last said on it.
#[derive(Clone)]
pub(crate) struct Feedback {
    pub(crate) toasts: ToastHost,
    /// The outcome said since the status line last spoke, if any — see
    /// [`Viewer::last_said`].
    outcome: Rc<RefCell<Option<String>>>,
    /// Every message said since each log was asked for.
    #[cfg(test)]
    logs: Rc<RefCell<Vec<SaidLog>>>,
}

impl Feedback {
    pub(crate) fn new() -> Self {
        Self {
            toasts: ToastHost::new(),
            outcome: Rc::default(),
            #[cfg(test)]
            logs: Rc::default(),
        }
    }
}

impl Viewer {
    /// Says `text` as a `notice`, on the channel that kind of message has.
    pub(crate) fn notify(&self, notice: Notice, text: &str) {
        #[cfg(test)]
        let before = self.last_said();
        match channel(notice) {
            Channel::StatusLine => {
                self.feedback.outcome.replace(None);
                self.status.set_text(text);
            }
            Channel::Toast(tone) => {
                self.feedback.outcome.replace(Some(text.to_owned()));
                self.status.set_text("");
                self.feedback.toasts.show(tone, text);
                // The status line asks for this itself whenever its text
                // changes (`tabs::connect`); an outcome off that line still
                // has to — "Saved" is exactly when a tab's unsaved marker
                // goes.
                super::tabs::schedule_sync(self);
            }
        }
        // Logged when it is not what was already showing — the status
        // label's own `notify::label` behaves the same way, and the logs
        // were first built on it.
        #[cfg(test)]
        if before != text {
            for log in self.feedback.logs.borrow().iter() {
                log.borrow_mut().push(text.to_owned());
            }
        }
    }

    /// Ambient information, on the status line.
    pub(crate) fn info(&self, text: &str) {
        self.notify(Notice::Info, text);
    }

    /// Something the user asked for is done.
    pub(crate) fn success(&self, text: &str) {
        self.notify(Notice::Success, text);
    }

    /// The shell declined to do something; nothing was changed or lost.
    pub(crate) fn refuse(&self, text: &str) {
        self.notify(Notice::Refusal, text);
    }

    /// An operation was attempted and failed.
    pub(crate) fn fail(&self, text: &str) {
        self.notify(Notice::Error, text);
    }

    /// The last message the shell said, whichever channel it went to: the
    /// outcome on the toast if one came after the status line last spoke,
    /// otherwise the status line.
    ///
    /// For a caller that has just been told "no" by something that reported
    /// why itself, and needs the same sentence for a place of its own — the
    /// palette's refusal, the signing picker's inline error. And for the
    /// tests, which assert on what was said without caring where.
    pub(crate) fn last_said(&self) -> glib::GString {
        match self.feedback.outcome.borrow().as_deref() {
            Some(outcome) => outcome.into(),
            None => self.status.text(),
        }
    }

    /// Every message said from now on, on either channel.
    #[cfg(test)]
    pub(crate) fn said_log(&self) -> SaidLog {
        let log = SaidLog::default();
        self.feedback.logs.borrow_mut().push(log.clone());
        log
    }
}
