//! How a bold / italic / underline toggle joins the queue, and how the other
//! edits of a run that has one queued do.
//!
//! A restyle may change what identifies its run once written — a switch to
//! a sibling face changes the font the run is painted in, a slant widens its
//! box — so the edits around it cannot be folded blindly the way repeated
//! retypes are:
//!
//! - **A second restyle of the same run** amends the queued one when that is
//!   the last queued edit naming the run: the queued run snapshot and `from`
//!   stay, `to` becomes the new look, and toggling a style back off before
//!   the save folds into writing nothing for it. The whole queue is replayed
//!   to check the amendment, since an edit queued after it may depend on
//!   what it used to write.
//! - **Anything else** — a first restyle, a restyle after another edit of the
//!   run, a retype or move of a restyled run — is checked against the page as
//!   the save will find it (every pending edit replayed first) and appended.
//!   When that fails for a run that already has a queued edit, the refusal is
//!   "save first", which is what lets the checkpoint write the queue and
//!   retry the edit against the result (`crate::checkpoint::record`).
//!
//! A run inside a queued batch is left to the batch rules in the parent
//! module, which fold a restyle into a restyle child (see `with_original`).

use pdf_document::{Command, Document, TextRun};
use pdf_manip::LopdfDocument;

use super::{amend, append, children, save_first, target, RecordError, Target};
use crate::bridge::ImportedSources;
use crate::content::{validate_content_command_after_pending, validate_pending_content_edits};

/// Records `command` when it is a restyle, or an edit of a run with a
/// restyle queued; `None` leaves it to the ordinary rules.
pub(super) fn record(
    document: &mut Document,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
    command: &Command,
) -> Option<Result<(), RecordError>> {
    let wanted = target(command)?;
    if in_queued_batch(document, wanted) {
        return None;
    }
    if let Command::RestyleTextRun { to, .. } = command {
        if let Some(index) = last_single_edit(document, wanted).filter(|index| {
            matches!(
                document.pending_edits.entries()[*index],
                Command::RestyleTextRun { .. }
            )
        }) {
            return Some(fold(document, base, sources, index, *to));
        }
        return Some(append_checked(document, base, sources, command, wanted));
    }
    let restyled = document.pending_edits.entries().iter().any(|entry| {
        matches!(entry, Command::RestyleTextRun { .. }) && target(entry) == Some(wanted)
    });
    restyled.then(|| append_checked(document, base, sources, command, wanted))
}

/// The queued restyle at `index` taking `to` as its new look.
fn fold(
    document: &mut Document,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
    index: usize,
    to: pdf_document::TextRunLook,
) -> Result<(), RecordError> {
    let Command::RestyleTextRun { item, from, .. } = &document.pending_edits.entries()[index]
    else {
        unreachable!("the caller found a restyle at this index");
    };
    let folded = Command::RestyleTextRun {
        item: item.clone(),
        from: *from,
        to,
    };
    let mut probe = document.clone();
    if !probe.pending_edits.amend(index, folded.clone()) {
        return Err(save_first());
    }
    validate_pending_content_edits(&probe, base, sources).map_err(|_| save_first())?;
    amend(document, index, folded)
}

/// Appends `command` once the page as the save will find it takes it.
fn append_checked(
    document: &mut Document,
    base: &LopdfDocument,
    sources: ImportedSources<'_, '_>,
    command: &Command,
    wanted: Target,
) -> Result<(), RecordError> {
    if let Err(error) = validate_content_command_after_pending(document, base, sources, command) {
        // An edit of a run another queued edit changed may only be stale; the
        // checkpoint can tell. One of an untouched run is simply impossible.
        return Err(if last_single_edit(document, wanted).is_some() {
            save_first()
        } else {
            error.into()
        });
    }
    append(document, command.clone())
}

/// The index of the last queued single edit naming `wanted`.
fn last_single_edit(document: &Document, wanted: Target) -> Option<usize> {
    document
        .pending_edits
        .entries()
        .iter()
        .rposition(|entry| !matches!(entry, Command::Batch(_)) && target(entry) == Some(wanted))
}

fn in_queued_batch(document: &Document, wanted: Target) -> bool {
    document.pending_edits.entries().iter().any(|entry| {
        matches!(entry, Command::Batch(_))
            && children(entry)
                .iter()
                .any(|child| target(child) == Some(wanted))
    })
}

/// A restyle child folded onto the queued one for the same run: the queued
/// snapshot and `from`, the new `to`. `None` unless both are restyles.
pub(super) fn with_original(next: &Command, earlier: &Command) -> Option<Command> {
    match (next, earlier) {
        (Command::RestyleTextRun { to, .. }, Command::RestyleTextRun { item, from, .. }) => {
            Some(restyle(item, *from, *to))
        }
        _ => None,
    }
}

fn restyle(
    item: &TextRun,
    from: pdf_document::TextRunLook,
    to: pdf_document::TextRunLook,
) -> Command {
    Command::RestyleTextRun {
        item: item.clone(),
        from,
        to,
    }
}
