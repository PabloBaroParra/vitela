//! Which toast is on screen, which wait behind it, and for how long each
//! stays. Pure: no widgets and no clock, so the policy is tested as values.

use std::time::Duration;

/// What kind of outcome a toast reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    /// Something the user asked for is done.
    Success,
    /// The shell declined to do something; nothing was changed or lost.
    Refusal,
    /// An operation was attempted and failed.
    Error,
}

/// One message and its tone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Toast {
    pub(crate) tone: Tone,
    pub(crate) text: String,
}

/// How long a success stays before reading time is added.
const SUCCESS_BASE: Duration = Duration::from_secs(4);
/// How long a refusal stays before reading time is added.
const REFUSAL_BASE: Duration = Duration::from_secs(8);
/// A message this short is read within the base time.
const FREE_CHARACTERS: usize = 40;
/// Reading time granted per character beyond [`FREE_CHARACTERS`].
const PER_CHARACTER: Duration = Duration::from_millis(50);
/// How many displaced errors wait to come back. One more drops the oldest.
pub(crate) const WAITING_LIMIT: usize = 3;

/// How long a toast stays up on its own, or `None` when it stays until it is
/// dismissed.
///
/// An error never leaves by itself: it reports work that did not happen, and
/// the user may be looking elsewhere when it arrives. A long message gets
/// reading time on top of its base, up to twice the base.
pub(crate) fn lifetime(tone: Tone, text: &str) -> Option<Duration> {
    let base = match tone {
        Tone::Success => SUCCESS_BASE,
        Tone::Refusal => REFUSAL_BASE,
        Tone::Error => return None,
    };
    let extra_characters = text.chars().count().saturating_sub(FREE_CHARACTERS);
    let extra = PER_CHARACTER.saturating_mul(u32::try_from(extra_characters).unwrap_or(u32::MAX));
    Some((base + extra).min(base * 2))
}

/// Whether a toast of this tone carries a close button. A success is gone
/// in seconds and needs none.
pub(crate) fn has_close_button(tone: Tone) -> bool {
    match tone {
        Tone::Success => false,
        Tone::Refusal | Tone::Error => true,
    }
}

/// What [`Queue::push`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pushed {
    /// The toast is now the one on screen.
    Shown,
    /// The same toast was already on screen; only its time starts over.
    Restarted,
}

/// The toast on screen and the errors it displaced.
///
/// The newest outcome is always the one shown: it answers what the user just
/// did. An error it pushed aside has not been dismissed, so it comes back
/// once the newer toast leaves — most recent first. A displaced success or
/// refusal is dropped: it was on its way out anyway.
#[derive(Default)]
pub(crate) struct Queue {
    current: Option<Toast>,
    /// Displaced errors, oldest first.
    waiting: Vec<Toast>,
}

impl Queue {
    pub(crate) fn current(&self) -> Option<&Toast> {
        self.current.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// Shows `toast` now.
    pub(crate) fn push(&mut self, toast: Toast) -> Pushed {
        if self.current.as_ref() == Some(&toast) {
            return Pushed::Restarted;
        }
        // Shown again now, so it must not also come back later.
        self.waiting.retain(|waiting| *waiting != toast);
        if let Some(displaced) = self.current.take() {
            if displaced.tone == Tone::Error {
                self.waiting.push(displaced);
                if self.waiting.len() > WAITING_LIMIT {
                    self.waiting.remove(0);
                }
            }
        }
        self.current = Some(toast);
        Pushed::Shown
    }

    /// Takes the toast on screen away — it timed out or was dismissed — and
    /// answers what shows next, if anything.
    pub(crate) fn dismiss(&mut self) -> Option<&Toast> {
        self.current = self.waiting.pop();
        self.current.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toast(tone: Tone, text: &str) -> Toast {
        Toast {
            tone,
            text: text.to_owned(),
        }
    }

    #[test]
    fn an_error_stays_until_dismissed_and_the_others_time_out() {
        assert_eq!(lifetime(Tone::Error, "Could not save."), None);
        assert_eq!(lifetime(Tone::Success, "Saved."), Some(SUCCESS_BASE));
        assert_eq!(lifetime(Tone::Refusal, "Not now."), Some(REFUSAL_BASE));
    }

    #[test]
    fn a_refusal_outlasts_a_success() {
        assert!(lifetime(Tone::Refusal, "x") > lifetime(Tone::Success, "x"));
    }

    #[test]
    fn a_long_message_gets_reading_time_up_to_twice_the_base() {
        let sixty = "x".repeat(FREE_CHARACTERS + 20);
        assert_eq!(
            lifetime(Tone::Success, &sixty),
            Some(SUCCESS_BASE + PER_CHARACTER * 20)
        );
        let endless = "x".repeat(10_000);
        assert_eq!(lifetime(Tone::Success, &endless), Some(SUCCESS_BASE * 2));
        assert_eq!(lifetime(Tone::Refusal, &endless), Some(REFUSAL_BASE * 2));
    }

    #[test]
    fn reading_time_counts_characters_not_bytes() {
        let accented = "ó".repeat(FREE_CHARACTERS);
        assert_eq!(lifetime(Tone::Success, &accented), Some(SUCCESS_BASE));
    }

    #[test]
    fn only_a_toast_that_lingers_has_a_close_button() {
        assert!(!has_close_button(Tone::Success));
        assert!(has_close_button(Tone::Refusal));
        assert!(has_close_button(Tone::Error));
    }

    #[test]
    fn the_newest_toast_is_the_one_shown() {
        let mut queue = Queue::default();
        assert_eq!(queue.current(), None);

        assert_eq!(queue.push(toast(Tone::Success, "Saved.")), Pushed::Shown);
        assert_eq!(queue.push(toast(Tone::Refusal, "Not now.")), Pushed::Shown);

        assert_eq!(queue.current(), Some(&toast(Tone::Refusal, "Not now.")));
    }

    #[test]
    fn the_same_toast_twice_only_restarts_its_time() {
        let mut queue = Queue::default();
        queue.push(toast(Tone::Error, "Could not save."));

        assert_eq!(
            queue.push(toast(Tone::Error, "Could not save.")),
            Pushed::Restarted
        );
        assert_eq!(queue.waiting(), 0);
        assert_eq!(queue.dismiss(), None, "it was one toast, not two");
    }

    #[test]
    fn the_same_text_in_another_tone_is_another_toast() {
        let mut queue = Queue::default();
        queue.push(toast(Tone::Refusal, "No."));

        assert_eq!(queue.push(toast(Tone::Error, "No.")), Pushed::Shown);
    }

    #[test]
    fn a_displaced_error_comes_back_once_the_newer_toast_leaves() {
        let mut queue = Queue::default();
        queue.push(toast(Tone::Error, "Could not save."));
        queue.push(toast(Tone::Success, "Exported."));

        assert_eq!(queue.current(), Some(&toast(Tone::Success, "Exported.")));
        assert_eq!(
            queue.dismiss(),
            Some(&toast(Tone::Error, "Could not save."))
        );
        assert_eq!(queue.dismiss(), None);
    }

    #[test]
    fn a_displaced_success_or_refusal_is_dropped() {
        let mut queue = Queue::default();
        queue.push(toast(Tone::Success, "Saved."));
        queue.push(toast(Tone::Refusal, "Not now."));
        queue.push(toast(Tone::Success, "Exported."));

        assert_eq!(queue.waiting(), 0);
        assert_eq!(queue.dismiss(), None);
    }

    #[test]
    fn displaced_errors_come_back_most_recent_first() {
        let mut queue = Queue::default();
        queue.push(toast(Tone::Error, "first"));
        queue.push(toast(Tone::Error, "second"));
        queue.push(toast(Tone::Error, "third"));

        assert_eq!(queue.dismiss(), Some(&toast(Tone::Error, "second")));
        assert_eq!(queue.dismiss(), Some(&toast(Tone::Error, "first")));
        assert_eq!(queue.dismiss(), None);
    }

    #[test]
    fn only_so_many_errors_wait_and_the_oldest_goes_first() {
        let mut queue = Queue::default();
        for index in 0..=WAITING_LIMIT + 1 {
            queue.push(toast(Tone::Error, &format!("error {index}")));
        }

        assert_eq!(queue.waiting(), WAITING_LIMIT);
        let mut back = Vec::new();
        while let Some(next) = queue.dismiss() {
            back.push(next.text.clone());
        }
        assert_eq!(back, ["error 3", "error 2", "error 1"]);
    }

    #[test]
    fn an_error_reported_again_is_shown_once_not_queued_twice() {
        let mut queue = Queue::default();
        queue.push(toast(Tone::Error, "Could not save."));
        queue.push(toast(Tone::Error, "Could not export."));
        queue.push(toast(Tone::Error, "Could not save."));

        assert_eq!(
            queue.current(),
            Some(&toast(Tone::Error, "Could not save."))
        );
        assert_eq!(
            queue.dismiss(),
            Some(&toast(Tone::Error, "Could not export."))
        );
        assert_eq!(queue.dismiss(), None);
    }
}
