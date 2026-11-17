//! Bold, italic, underline and strikethrough toggles on existing text
//! through the FFI: the look a shell's buttons show, and the restyles a
//! press records.
//!
//! What a restyle writes — a sibling face or a synthesized one, and whether a
//! natively bold run can be made regular at all — is `pdf_edit::restyle`'s;
//! how it joins the queue is `pdf_save::record`'s. One restyle goes through
//! `apply_edit` as `FfiEditCommand::RestyleTextRun`; several (a block, a
//! multi-selection) go through [`restyle_text_runs`] as one undo step.

use pdf_document::{Command, TextRunLook};

use crate::document::{apply_edit, DocumentHandle};
use crate::error::FfiError;
use crate::text_blocks::apply_batch_edit;
use crate::types::{FfiContentTextRun, FfiEditCommand};

/// Mirrors `pdf_document::TextRunLook`: what the reader should see, not how
/// it is painted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, uniffi::Record)]
pub struct FfiTextRunLook {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

impl From<FfiTextRunLook> for TextRunLook {
    fn from(look: FfiTextRunLook) -> Self {
        Self {
            bold: look.bold,
            italic: look.italic,
            underline: look.underline,
            strike: look.strike,
        }
    }
}

impl From<TextRunLook> for FfiTextRunLook {
    fn from(look: TextRunLook) -> Self {
        Self {
            bold: look.bold,
            italic: look.italic,
            underline: look.underline,
            strike: look.strike,
        }
    }
}

/// How a run looks now, and which styles can be taken off it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct FfiTextRunLookState {
    pub look: FfiTextRunLook,
    /// `false` only for a bold run painted in a bold font whose page has no
    /// regular sibling: un-bolding is never faked. `true` for a plain run.
    pub can_remove_bold: bool,
    /// The same, for italic.
    pub can_remove_italic: bool,
}

/// One run's restyle inside [`restyle_text_runs`].
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiTextRunRestyle {
    /// The run exactly as `read_page_content` read it.
    pub item: FfiContentTextRun,
    /// How it looked when the toggle was pressed.
    pub from: FfiTextRunLook,
    /// How it should look.
    pub to: FfiTextRunLook,
}

/// How `run` looks with every pending edit in place — a restyle queued but
/// not saved yet included (`pdf_save::pending_text_run_look_of`). `None`
/// when its page has no content.
#[uniffi::export]
pub fn text_run_look(
    handle: &DocumentHandle,
    run: FfiContentTextRun,
) -> Result<Option<FfiTextRunLookState>, FfiError> {
    let state = handle.lock();
    let run = state.text_run(run)?;
    let look = state.with_imported_sources(|sources| {
        pdf_save::pending_text_run_look_of(state.document(), state.base(), sources, &run)
    })?;
    Ok(look.map(|look| FfiTextRunLookState {
        look: look.look.into(),
        can_remove_bold: look.can_remove_bold,
        can_remove_italic: look.can_remove_italic,
    }))
}

/// Restyles every run of `restyles` as one undo step. One entry is an
/// ordinary restyle, recorded exactly as `apply_edit` records it (and folded
/// into a restyle of the same run queued before it); none records nothing.
#[uniffi::export]
pub fn restyle_text_runs(
    handle: &DocumentHandle,
    mut restyles: Vec<FfiTextRunRestyle>,
) -> Result<(), FfiError> {
    match restyles.len() {
        0 => Ok(()),
        1 => {
            let FfiTextRunRestyle { item, from, to } = restyles.remove(0);
            apply_edit(handle, FfiEditCommand::RestyleTextRun { item, from, to })
        }
        _ => apply_batch_edit(handle, |state| {
            restyles
                .into_iter()
                .map(|restyle| {
                    Ok(Command::RestyleTextRun {
                        item: state.text_run(restyle.item)?,
                        from: restyle.from.into(),
                        to: restyle.to.into(),
                    })
                })
                .collect::<Result<Vec<_>, FfiError>>()
                .map(Command::Batch)
        }),
    }
}
