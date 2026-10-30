//! In-memory checkpoints: the "save and reopen" a reader used to do by hand,
//! done by the core when an edit needs it.
//!
//! Every queued page-content edit is checked against `base`, the file as it
//! was opened. An item an earlier queued edit changed is therefore not there
//! to be found: a retyped run moved, a moved run retyped, a pending insertion
//! deleted. Each of those was refused with "save and reopen first", and after
//! the reader did that the edit worked, because the save had become the new
//! base.
//!
//! A checkpoint is that save, kept in memory. The queued edits are written
//! into new bytes, the bytes are parsed into the new `base`, and the queue
//! starts empty. The refused edit is then retried against the new base, with
//! its target **rebound** to the item the new parse holds: the item a shell
//! sends is the one `read_page_content` overlaid on the old base, whose box is
//! partly an estimate and whose id may be synthetic.
//!
//! **Undo walks through a checkpoint.** The old base, its bytes and its queue
//! are kept. An undo that finds the queue empty steps back to them and undoes
//! the last queued edit there, so the reader sees one step at a time, exactly
//! as without the checkpoint. A redo that runs out steps forward again, but
//! only to the state it left: any other edit in between drops that future, as
//! a fresh edit always drops the redo stack.
//!
//! **What may checkpoint.** Only a session whose queue holds page-content
//! edits and nothing else, on an unencrypted, unsigned file opened from bytes,
//! whose pages are still the file's own pages in the file's own order. Those
//! are the sessions where the model holds no state a rewrite could renumber
//! from under it. Anything else keeps today's refusal, which still tells the
//! reader to save first. The checks after the rewrite (pages and form fields
//! read back identically) are the same rule enforced, not assumed.

use pdf_document::{EditLog, ImageItem, PageContent, PathItem, Rect};

use super::*;

/// One step back: the base an edit queue was recorded against, and that
/// queue.
pub(super) struct Checkpoint {
    base: LopdfDocument,
    original_bytes: Vec<u8>,
    log: EditLog,
    /// For a step forward only: the queue the session must still hold for
    /// the step to be the one it left. See [`DocumentState::redo`].
    resume_log: Option<EditLog>,
    /// For a checkpoint just taken only: the aliases it replaced, put back
    /// if the retried edit fails after all.
    aliases: Vec<(Item, Item)>,
}

/// The checkpoints behind and ahead of the live state.
#[derive(Default)]
pub(super) struct Checkpoints {
    behind: Vec<Checkpoint>,
    ahead: Vec<Checkpoint>,
    /// Bumped whenever `base` is swapped, so a shell knows that every item
    /// it read before is stale. See [`DocumentHandle::content_base_generation`].
    generation: u64,
    /// What the last checkpoint rebound, as `(held, rebound)`. A shell may go
    /// on sending the item it held — the Windows editor resends the run it
    /// opened over at every pause in the typing — and that item names the
    /// old base. Mapped here, it reaches the edit the checkpoint queued and
    /// folds into it, instead of becoming a second edit of the same item,
    /// which no save could replay. Valid only against the base the
    /// checkpoint produced, so every other swap of `base` clears it.
    aliases: Vec<(Item, Item)>,
}

/// One page-content item, whatever its kind.
#[derive(Clone, PartialEq)]
enum Item {
    Run(TextRun),
    Image(ImageItem),
    Path(PathItem),
}

/// How far a rebound item may sit from the item the caller held. A retyped
/// run's overlaid box keeps its origin but estimates its width, and a moved
/// item lands where the move put it, so the origin is what identifies it.
const REBIND_TOLERANCE: f64 = 2.0;

impl DocumentState {
    /// Retries `command` through `apply` after a checkpoint, when the
    /// session may take one. `apply` is the same recording path that just
    /// refused it: a single edit's or a batch's.
    ///
    /// Returns `refusal` and leaves the session exactly as it was when the
    /// session may not checkpoint, or when the edit fails against the new
    /// base too.
    pub(crate) fn retry_after_checkpoint(
        &mut self,
        command: Command,
        refusal: FfiError,
        apply: fn(&mut DocumentState, Command) -> Result<(), FfiError>,
    ) -> Result<(), FfiError> {
        if !self.may_checkpoint()
            || !self.targets_are_current(&command)
            || self.take_checkpoint().is_err()
        {
            return Err(refusal);
        }
        let rebound = self.rebind(command.clone());
        let aliases: Vec<(Item, Item)> = targets(&command)
            .into_iter()
            .zip(targets(&rebound))
            .map(|(held, rebound)| (held.to_item(), rebound.to_item()))
            .filter(|(held, rebound)| held != rebound)
            .collect();
        match apply(self, rebound) {
            Ok(()) => {
                let checkpoint = self
                    .checkpoints
                    .behind
                    .last_mut()
                    .expect("a checkpoint was just taken");
                // The retried edit is a fresh edit, so what the old queue
                // could still redo is no longer a future of this session.
                checkpoint.log.discard_redo();
                self.checkpoints.ahead.clear();
                self.checkpoints.aliases = aliases;
                self.checkpoints.generation += 1;
                Ok(())
            }
            Err(_) => {
                let behind = self
                    .checkpoints
                    .behind
                    .pop()
                    .expect("a checkpoint was just taken");
                let aliases = behind.aliases.clone();
                self.swap_in(behind);
                self.checkpoints.aliases = aliases;
                Err(refusal)
            }
        }
    }

    /// `command` with every item the last checkpoint rebound replaced by
    /// what it was rebound to. See [`Checkpoints::aliases`].
    pub(crate) fn dealias(&self, command: Command) -> Command {
        let aliases = &self.checkpoints.aliases;
        if aliases.is_empty() {
            return command;
        }
        map_targets(command, &mut |item| {
            aliases
                .iter()
                .find(|(held, _)| *held == item)
                .map_or(item, |(_, rebound)| rebound.clone())
        })
    }

    pub(super) fn can_undo(&self) -> bool {
        self.document.pending_edits.can_undo() || !self.checkpoints.behind.is_empty()
    }

    pub(super) fn can_redo(&self) -> bool {
        self.document.pending_edits.can_redo() || self.can_step_forward()
    }

    pub(super) fn content_base_generation(&self) -> u64 {
        self.checkpoints.generation
    }

    pub(super) fn undo(&mut self) -> bool {
        if self.document.pending_edits.can_undo() {
            return step_log(&mut self.document, EditLog::undo);
        }
        let Some(behind) = self.checkpoints.behind.pop() else {
            return false;
        };
        let resume_log = behind.log.clone();
        let current = self.swap_in(behind);
        self.checkpoints.generation += 1;
        self.checkpoints.ahead.push(Checkpoint {
            resume_log: Some(resume_log),
            ..current
        });
        step_log(&mut self.document, EditLog::undo)
    }

    pub(super) fn redo(&mut self) -> bool {
        if self.document.pending_edits.can_redo() {
            return step_log(&mut self.document, EditLog::redo);
        }
        if !self.can_step_forward() {
            self.checkpoints.ahead.clear();
            return false;
        }
        let ahead = self.checkpoints.ahead.pop().expect("checked above");
        let current = self.swap_in(ahead);
        self.checkpoints.generation += 1;
        self.checkpoints.behind.push(current);
        step_log(&mut self.document, EditLog::redo)
    }

    /// Whether the checkpoint ahead is the one this session stepped back
    /// from, with nothing done since.
    fn can_step_forward(&self) -> bool {
        self.checkpoints
            .ahead
            .last()
            .is_some_and(|ahead| ahead.resume_log.as_ref() == Some(&self.document.pending_edits))
    }

    pub(super) fn may_checkpoint(&self) -> bool {
        let entries = self.document.pending_edits.entries();
        self.original_bytes.is_some()
            && self.document.security.is_none()
            && self.base.as_lopdf().encryption_state.is_none()
            && self.imported.is_empty()
            && !entries.is_empty()
            && entries.iter().all(Command::is_content_edit)
            && !pdf_manip::document_has_signatures(&self.base)
            && pdf_save::populate_document(&self.base)
                .is_ok_and(|pages| pages == self.document.pages)
    }

    /// Writes the queue into a new base and empties the queue, keeping what
    /// it replaced as the newest checkpoint behind.
    fn take_checkpoint(&mut self) -> Result<(), FfiError> {
        let bytes = self.with_save_input(
            FfiSaveIntent::Default,
            FfiSignatureAcknowledgement::Unacknowledged,
            pdf_save::save_document,
        )?;
        let (base, _) = pdf_manip::open_document_from_bytes(&bytes, None)?;
        let reread = pdf_save::document_from_lopdf(&base, None)?;
        if reread.pages != self.document.pages || reread.form_fields != self.document.form_fields {
            return Err(FfiError::UnsupportedOperation {
                detail: "the edits could not be written into a new base".to_string(),
            });
        }
        let checkpoint = self.swap_in(Checkpoint {
            base,
            original_bytes: bytes,
            log: EditLog::new(),
            resume_log: None,
            aliases: Vec::new(),
        });
        self.checkpoints.behind.push(checkpoint);
        Ok(())
    }

    /// Makes `checkpoint` the live base and queue, handing back the ones it
    /// replaced, aliases included.
    fn swap_in(&mut self, checkpoint: Checkpoint) -> Checkpoint {
        let base = std::mem::replace(&mut self.base, checkpoint.base);
        let original_bytes = self
            .original_bytes
            .replace(checkpoint.original_bytes)
            .expect("a checkpointed session has original bytes");
        let log = std::mem::replace(&mut self.document.pending_edits, checkpoint.log);
        let aliases = std::mem::take(&mut self.checkpoints.aliases);
        Checkpoint {
            base,
            original_bytes,
            log,
            resume_log: None,
            aliases,
        }
    }

    /// Whether every item `command` targets is one the page shows right now,
    /// as `read_page_content` hands it out.
    ///
    /// Rebinding finds an item by what it shows and where, which cannot tell
    /// a stale snapshot from the live item that took its place: an undone
    /// insertion and the next insertion, typed at the same spot, look the
    /// same. The id and font a shell read are what still tell them apart, so
    /// they are checked here, before the queue that defines them is written
    /// away.
    fn targets_are_current(&self, command: &Command) -> bool {
        let mut pages: HashMap<PageId, Option<PageContent>> = HashMap::new();
        let mut current = |page: PageId| -> Option<PageContent> {
            pages
                .entry(page)
                .or_insert_with(|| {
                    self.with_imported_sources(|sources| {
                        pdf_save::read_pending_page_content_of(
                            &self.document,
                            page,
                            &self.base,
                            sources,
                        )
                    })
                    .ok()
                })
                .clone()
        };
        targets(command).into_iter().all(|target| match target {
            Target::Run(held) => current(held.page).is_some_and(|page| {
                page.text_runs.iter().any(|run| {
                    run.id == held.id
                        && run.text == held.text
                        && run.resource_font_name == held.resource_font_name
                })
            }),
            Target::Image(held) => current(held.page).is_some_and(|page| {
                page.images
                    .iter()
                    .any(|image| image.id == held.id && image.source == held.source)
            }),
            Target::Path(held) => current(held.page).is_some_and(|page| {
                page.paths
                    .iter()
                    .any(|path| path.id == held.id && path.paint == held.paint)
            }),
        })
    }

    /// `command` with every target replaced by the item the live base holds
    /// for it. A target that cannot be told apart is left as it was, and the
    /// edit then fails validation instead of landing on the wrong item.
    fn rebind(&self, command: Command) -> Command {
        let mut pages: HashMap<PageId, Option<PageContent>> = HashMap::new();
        let mut content = |page: PageId| -> Option<PageContent> {
            pages
                .entry(page)
                .or_insert_with(|| {
                    self.with_imported_sources(|sources| {
                        pdf_save::read_page_content_of(&self.document, page, &self.base, sources)
                    })
                    .ok()
                })
                .clone()
        };
        map_targets(command, &mut |held| rebind_item(held, &mut content))
    }
}

/// The item the live base holds for `held`, found by what it shows and
/// where; `held` itself when there is not exactly one.
fn rebind_item(held: Item, content: &mut impl FnMut(PageId) -> Option<PageContent>) -> Item {
    let found = match &held {
        Item::Run(item) => content(item.page).and_then(|page| {
            nearest(
                &page.text_runs,
                item,
                |run| run.text == item.text,
                |run| run.bbox,
            )
            .map(Item::Run)
        }),
        Item::Image(item) => content(item.page).and_then(|page| {
            nearest(
                &page.images,
                item,
                |image| image.source == item.source,
                |image| image.bbox,
            )
            .map(Item::Image)
        }),
        Item::Path(item) => content(item.page).and_then(|page| {
            nearest(
                &page.paths,
                item,
                |path| path.paint == item.paint,
                |path| path.bbox,
            )
            .map(Item::Path)
        }),
    };
    found.unwrap_or(held)
}

/// `command` with every item it targets passed through `map`. A move of a
/// text run keeps its displacement from wherever the run is mapped to.
fn map_targets(command: Command, map: &mut dyn FnMut(Item) -> Item) -> Command {
    match command {
        Command::ReplaceTextRunContent { item, after } => Command::ReplaceTextRunContent {
            item: run(item, map),
            after,
        },
        Command::ReplaceTextRunWithInsertedFont { item, after } => {
            Command::ReplaceTextRunWithInsertedFont {
                item: run(item, map),
                after,
            }
        }
        Command::RemoveTextRun(item) => Command::RemoveTextRun(run(item, map)),
        Command::MoveTextRun { item, to } => {
            let held = item.bbox;
            let item = run(item, map);
            // A move is a displacement: keep it, from wherever the run
            // really starts.
            let to = Rect {
                x: to.x + item.bbox.x - held.x,
                y: to.y + item.bbox.y - held.y,
                width: item.bbox.width,
                height: item.bbox.height,
            };
            Command::MoveTextRun { item, to }
        }
        Command::RemoveImage { item, source } => Command::RemoveImage {
            item: image(item, map),
            source,
        },
        Command::MoveImage { item, to } => Command::MoveImage {
            item: image(item, map),
            to,
        },
        Command::ResizeImage { item, to } => Command::ResizeImage {
            item: image(item, map),
            to,
        },
        Command::ReplaceImageSource {
            item,
            before,
            after,
        } => Command::ReplaceImageSource {
            item: image(item, map),
            before,
            after,
        },
        Command::MovePath { item, to } => Command::MovePath {
            item: path(item, map),
            to,
        },
        Command::ResizePath { item, to } => Command::ResizePath {
            item: path(item, map),
            to,
        },
        Command::RemovePath(item) => Command::RemovePath(path(item, map)),
        Command::Batch(children) => Command::Batch(
            children
                .into_iter()
                .map(|child| map_targets(child, map))
                .collect(),
        ),
        other => other,
    }
}

fn run(item: TextRun, map: &mut dyn FnMut(Item) -> Item) -> TextRun {
    match map(Item::Run(item.clone())) {
        Item::Run(mapped) => mapped,
        _ => item,
    }
}

fn image(item: ImageItem, map: &mut dyn FnMut(Item) -> Item) -> ImageItem {
    match map(Item::Image(item.clone())) {
        Item::Image(mapped) => mapped,
        _ => item,
    }
}

fn path(item: PathItem, map: &mut dyn FnMut(Item) -> Item) -> PathItem {
    match map(Item::Path(item.clone())) {
        Item::Path(mapped) => mapped,
        _ => item,
    }
}

enum Target<'a> {
    Run(&'a TextRun),
    Image(&'a ImageItem),
    Path(&'a PathItem),
}

impl Target<'_> {
    fn to_item(&self) -> Item {
        match self {
            Target::Run(item) => Item::Run((*item).clone()),
            Target::Image(item) => Item::Image((*item).clone()),
            Target::Path(item) => Item::Path((*item).clone()),
        }
    }
}

/// Every item `command` names. Empty for an edit that names none (an
/// insertion, a paste, a watermark), which has nothing to go stale.
fn targets(command: &Command) -> Vec<Target<'_>> {
    match command {
        Command::ReplaceTextRunContent { item, .. }
        | Command::ReplaceTextRunWithInsertedFont { item, .. }
        | Command::RemoveTextRun(item)
        | Command::MoveTextRun { item, .. } => vec![Target::Run(item)],
        Command::RemoveImage { item, .. }
        | Command::MoveImage { item, .. }
        | Command::ResizeImage { item, .. }
        | Command::ReplaceImageSource { item, .. } => vec![Target::Image(item)],
        Command::MovePath { item, .. }
        | Command::ResizePath { item, .. }
        | Command::RemovePath(item) => vec![Target::Path(item)],
        Command::Batch(children) => children.iter().flat_map(targets).collect(),
        _ => Vec::new(),
    }
}

/// The one candidate `same` accepts whose origin is within
/// [`REBIND_TOLERANCE`] of `held`'s. `None` when there is none, or more than
/// one: two lookalikes at one spot are a guess this refuses to make.
fn nearest<T: Clone>(
    candidates: &[T],
    held: &T,
    same: impl Fn(&T) -> bool,
    bbox: impl Fn(&T) -> Rect,
) -> Option<T> {
    let origin = bbox(held);
    let mut close = candidates.iter().filter(|candidate| {
        let rect = bbox(candidate);
        same(candidate)
            && (rect.x - origin.x).abs() <= REBIND_TOLERANCE
            && (rect.y - origin.y).abs() <= REBIND_TOLERANCE
    });
    match (close.next(), close.next()) {
        (Some(only), None) => Some(only.clone()),
        _ => None,
    }
}

fn step_log(document: &mut Document, step: fn(&mut EditLog, &mut Document) -> bool) -> bool {
    let mut log = std::mem::take(&mut document.pending_edits);
    let stepped = step(&mut log, document);
    document.pending_edits = log;
    stepped
}
