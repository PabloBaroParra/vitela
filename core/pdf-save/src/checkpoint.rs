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
//! sends is the one [`crate::read_pending_page_content_of`] overlaid on the
//! old base, whose box is partly an estimate and whose id may be synthetic.
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
//! with no imported PDFs, whose pages are still the file's own pages in the
//! file's own order. Those are the sessions where the model holds no state a
//! rewrite could renumber from under it. Anything else keeps today's refusal,
//! which still tells the reader to save first. The checks after the rewrite
//! (pages and form fields read back identically) are the same rule enforced,
//! not assumed.
//!
//! This used to live in `pdf-ffi`, where only the shells behind the FFI could
//! reach it. A shell owns its session however it likes — the FFI keeps the
//! parse and the bytes as two fields, the GTK shell keeps them behind `Arc`s
//! with a password beside them — so the core works on it through
//! [`CheckpointSession`] and stores whatever the shell calls a base.

use std::collections::HashMap;

use pdf_document::{
    Command, Document, EditLog, ImageItem, PageContent, PageId, PathItem, Rect, TextRun,
};
use pdf_manip::LopdfDocument;

use crate::bridge::{document_from_lopdf, populate_document, ImportedSources};
use crate::origin::{read_page_content_of, read_pending_page_content_of};
use crate::security::SaveIntent;
use crate::strategy::{save_document, SaveInput, SignatureAcknowledgement};

/// What the core needs of a shell's editing session to checkpoint it.
///
/// `Base` is whatever the shell keeps as one base: at least the parse and the
/// bytes it came from. The core never looks inside one; it asks the shell to
/// build one ([`Self::checkpointed_base`]) and to swap one in
/// ([`Self::replace_base`]).
pub trait CheckpointSession {
    type Base;

    fn document(&self) -> &Document;
    fn document_mut(&mut self) -> &mut Document;
    /// The parse every queued page-content edit is checked against.
    fn base(&self) -> &LopdfDocument;
    /// The bytes [`Self::base`] was parsed from; `None` for a document
    /// created in memory with nothing yet saved.
    fn original_bytes(&self) -> Option<&[u8]>;
    /// Whether this session holds any imported PDF. One that does may not
    /// checkpoint, even after an undo took the imported pages back out: a
    /// redo would put them back.
    fn has_imported_sources(&self) -> bool;
    fn checkpoints(&self) -> &Checkpoints<Self::Base>;
    fn checkpoints_mut(&mut self) -> &mut Checkpoints<Self::Base>;
    /// The base a checkpoint wrote, from its parse and its bytes. Whatever
    /// else the shell keeps with a base is carried over from the live one.
    fn checkpointed_base(&self, base: LopdfDocument, bytes: Vec<u8>) -> Self::Base;
    /// Makes `base` the live one, handing back the one it replaced.
    fn replace_base(&mut self, base: Self::Base) -> Self::Base;
}

/// One step back: the base an edit queue was recorded against, and that
/// queue.
struct Checkpoint<B> {
    base: B,
    log: EditLog,
    /// For a step forward only: the queue the session must still hold for
    /// the step to be the one it left. See [`redo`].
    resume_log: Option<EditLog>,
    /// For a checkpoint just taken only: the aliases it replaced, put back
    /// if the retried edit fails after all.
    aliases: Vec<(Item, Item)>,
}

/// The checkpoints behind and ahead of a session's live state. A shell keeps
/// one beside its base, and starts a fresh one whenever it opens a file.
pub struct Checkpoints<B> {
    behind: Vec<Checkpoint<B>>,
    ahead: Vec<Checkpoint<B>>,
    /// Bumped whenever the base is swapped, so a shell knows that every item
    /// it read before is stale. See [`Checkpoints::generation`].
    generation: u64,
    /// What the last checkpoint rebound, as `(held, rebound)`. A shell may go
    /// on sending the item it held — the Windows editor resends the run it
    /// opened over at every pause in the typing — and that item names the
    /// old base. Mapped here, it reaches the edit the checkpoint queued and
    /// folds into it, instead of becoming a second edit of the same item,
    /// which no save could replay. Valid only against the base the
    /// checkpoint produced, so every other swap of the base clears it.
    aliases: Vec<(Item, Item)>,
}

impl<B> Default for Checkpoints<B> {
    fn default() -> Self {
        Self {
            behind: Vec::new(),
            ahead: Vec::new(),
            generation: 0,
            aliases: Vec::new(),
        }
    }
}

impl<B> Checkpoints<B> {
    /// Changes whenever a checkpoint is taken, or undo or redo steps across
    /// one. Every content item read before the change is stale: re-read the
    /// pages before building another content edit from them.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether `live`, the session's queue, or a checkpoint behind it has
    /// anything to undo.
    pub fn can_undo(&self, live: &EditLog) -> bool {
        live.can_undo() || !self.behind.is_empty()
    }

    /// Whether `live`, the session's queue, or the checkpoint ahead of it
    /// has anything to redo.
    pub fn can_redo(&self, live: &EditLog) -> bool {
        live.can_redo() || self.can_step_forward(live)
    }

    /// The command [`undo`] would undo, read without stepping — in the queue
    /// behind the last checkpoint when `live` has nothing to undo.
    pub fn peek_undo<'a>(&'a self, live: &'a EditLog) -> Option<&'a Command> {
        if live.can_undo() {
            return live.peek_undo();
        }
        self.behind.last().and_then(|behind| behind.log.peek_undo())
    }

    /// The command [`redo`] would redo, read without stepping.
    pub fn peek_redo<'a>(&'a self, live: &'a EditLog) -> Option<&'a Command> {
        if live.can_redo() {
            return live.peek_redo();
        }
        if !self.can_step_forward(live) {
            return None;
        }
        self.ahead.last().and_then(|ahead| ahead.log.peek_redo())
    }

    /// Whether the checkpoint ahead is the one the session stepped back from,
    /// with nothing done since: `live` is still the queue it left.
    fn can_step_forward(&self, live: &EditLog) -> bool {
        self.ahead
            .last()
            .is_some_and(|ahead| ahead.resume_log.as_ref() == Some(live))
    }

    /// `command` with every item the last checkpoint rebound replaced by
    /// what it was rebound to. See [`Checkpoints::aliases`].
    pub fn dealias(&self, command: Command) -> Command {
        if self.aliases.is_empty() {
            return command;
        }
        map_targets(command, &mut |item| {
            self.aliases
                .iter()
                .find(|(held, _)| *held == item)
                .map_or(item, |(_, rebound)| rebound.clone())
        })
    }
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

/// Records `command` through `apply`, checkpointing once when `apply` refuses
/// it and the session may take one.
///
/// The command is dealiased first ([`Checkpoints::dealias`]) and tried as it
/// stands: a checkpoint costs a rewrite, and most edits never need one.
/// `apply` is the recording path for this kind of edit — a single edit's or a
/// batch's — and must leave the session untouched when it refuses.
pub fn record<S: CheckpointSession, E>(
    session: &mut S,
    command: Command,
    mut apply: impl FnMut(&mut S, Command) -> Result<(), E>,
) -> Result<(), E> {
    let command = session.checkpoints().dealias(command);
    match apply(session, command.clone()) {
        Ok(()) => Ok(()),
        Err(refusal) => retry_after_checkpoint(session, command, refusal, apply),
    }
}

/// Retries `command` through `apply` after a checkpoint, when the session may
/// take one. `apply` is the same recording path that just refused it.
///
/// Returns `refusal` and leaves the session exactly as it was when the
/// session may not checkpoint, or when the edit fails against the new base
/// too.
pub fn retry_after_checkpoint<S: CheckpointSession, E>(
    session: &mut S,
    command: Command,
    refusal: E,
    mut apply: impl FnMut(&mut S, Command) -> Result<(), E>,
) -> Result<(), E> {
    if !may_checkpoint(session)
        || !targets_are_current(session, &command)
        || take_checkpoint(session).is_none()
    {
        return Err(refusal);
    }
    let rebound = rebind(session, command.clone());
    let aliases: Vec<(Item, Item)> = targets(&command)
        .into_iter()
        .zip(targets(&rebound))
        .map(|(held, rebound)| (held.to_item(), rebound.to_item()))
        .filter(|(held, rebound)| held != rebound)
        .collect();
    match apply(session, rebound) {
        Ok(()) => {
            let checkpoints = session.checkpoints_mut();
            let checkpoint = checkpoints
                .behind
                .last_mut()
                .expect("a checkpoint was just taken");
            // The retried edit is a fresh edit, so what the old queue could
            // still redo is no longer a future of this session.
            checkpoint.log.discard_redo();
            checkpoints.ahead.clear();
            checkpoints.aliases = aliases;
            checkpoints.generation += 1;
            Ok(())
        }
        Err(_) => {
            let behind = session
                .checkpoints_mut()
                .behind
                .pop()
                .expect("a checkpoint was just taken");
            let aliases = behind.aliases.clone();
            swap_in(session, behind);
            session.checkpoints_mut().aliases = aliases;
            Err(refusal)
        }
    }
}

/// Whether the session has anything to undo, a checkpoint behind included.
pub fn can_undo<S: CheckpointSession>(session: &S) -> bool {
    session
        .checkpoints()
        .can_undo(&session.document().pending_edits)
}

/// Whether the session has anything to redo, a checkpoint ahead included.
pub fn can_redo<S: CheckpointSession>(session: &S) -> bool {
    session
        .checkpoints()
        .can_redo(&session.document().pending_edits)
}

/// Undoes the last edit, stepping back across a checkpoint when the live
/// queue is empty. Returns whether anything was undone.
pub fn undo<S: CheckpointSession>(session: &mut S) -> bool {
    if session.document().pending_edits.can_undo() {
        return step_log(session.document_mut(), EditLog::undo);
    }
    let Some(behind) = session.checkpoints_mut().behind.pop() else {
        return false;
    };
    let resume_log = behind.log.clone();
    let current = swap_in(session, behind);
    let checkpoints = session.checkpoints_mut();
    checkpoints.generation += 1;
    checkpoints.ahead.push(Checkpoint {
        resume_log: Some(resume_log),
        ..current
    });
    step_log(session.document_mut(), EditLog::undo)
}

/// Redoes the last undone edit, stepping forward across a checkpoint when
/// the live queue has nothing to redo and nothing was done since the step
/// back. Returns whether anything was redone.
pub fn redo<S: CheckpointSession>(session: &mut S) -> bool {
    if session.document().pending_edits.can_redo() {
        return step_log(session.document_mut(), EditLog::redo);
    }
    if !session
        .checkpoints()
        .can_step_forward(&session.document().pending_edits)
    {
        session.checkpoints_mut().ahead.clear();
        return false;
    }
    let ahead = session
        .checkpoints_mut()
        .ahead
        .pop()
        .expect("checked above");
    let current = swap_in(session, ahead);
    let checkpoints = session.checkpoints_mut();
    checkpoints.generation += 1;
    checkpoints.behind.push(current);
    step_log(session.document_mut(), EditLog::redo)
}

/// Whether a page-content edit refused because its item already has an
/// unsaved edit would instead be written into a new in-memory base and
/// retried. `false` means the old rule holds: save and reopen first.
pub fn may_checkpoint<S: CheckpointSession>(session: &S) -> bool {
    can_checkpoint(
        session.document(),
        session.base(),
        session.original_bytes().is_some(),
        session.has_imported_sources(),
    )
}

/// [`may_checkpoint`] for a caller holding the session's parts rather than a
/// [`CheckpointSession`]: whether `document`, queued against `base`, may be
/// written into a new base. `has_original_bytes` and `has_imported_sources`
/// are [`CheckpointSession::original_bytes`] being `Some` and
/// [`CheckpointSession::has_imported_sources`].
pub fn can_checkpoint(
    document: &Document,
    base: &LopdfDocument,
    has_original_bytes: bool,
    has_imported_sources: bool,
) -> bool {
    let entries = document.pending_edits.entries();
    has_original_bytes
        && document.security.is_none()
        && base.as_lopdf().encryption_state.is_none()
        && !has_imported_sources
        && !entries.is_empty()
        && entries.iter().all(Command::is_content_edit)
        && !pdf_manip::document_has_signatures(base)
        && populate_document(base).is_ok_and(|pages| pages == document.pages)
}

/// Writes the queue into a new base and empties the queue, keeping what it
/// replaced as the newest checkpoint behind. `None` when the rewrite fails or
/// does not read back as the same pages and form fields.
fn take_checkpoint<S: CheckpointSession>(session: &mut S) -> Option<()> {
    // `may_checkpoint` already ruled out imported PDFs, so there are none to
    // hand the save.
    let bytes = save_document(SaveInput {
        document: session.document(),
        base: session.base(),
        original_bytes: session.original_bytes(),
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
        imported_sources: ImportedSources::none(),
    })
    .ok()?;
    let (base, _) = pdf_manip::open_document_from_bytes(&bytes, None).ok()?;
    let reread = document_from_lopdf(&base, None).ok()?;
    let document = session.document();
    if reread.pages != document.pages || reread.form_fields != document.form_fields {
        return None;
    }
    let base = session.checkpointed_base(base, bytes);
    let checkpoint = swap_in(
        session,
        Checkpoint {
            base,
            log: EditLog::new(),
            resume_log: None,
            aliases: Vec::new(),
        },
    );
    session.checkpoints_mut().behind.push(checkpoint);
    Some(())
}

/// Makes `checkpoint` the live base and queue, handing back the ones it
/// replaced, aliases included.
fn swap_in<S: CheckpointSession>(
    session: &mut S,
    checkpoint: Checkpoint<S::Base>,
) -> Checkpoint<S::Base> {
    let base = session.replace_base(checkpoint.base);
    let log = std::mem::replace(&mut session.document_mut().pending_edits, checkpoint.log);
    let aliases = std::mem::take(&mut session.checkpoints_mut().aliases);
    Checkpoint {
        base,
        log,
        resume_log: None,
        aliases,
    }
}

/// Whether every item `command` targets is one the page shows right now, as
/// [`read_pending_page_content_of`] hands it out.
///
/// Rebinding finds an item by what it shows and where, which cannot tell a
/// stale snapshot from the live item that took its place: an undone
/// insertion and the next insertion, typed at the same spot, look the same.
/// The id and font a shell read are what still tell them apart, so they are
/// checked here, before the queue that defines them is written away.
fn targets_are_current<S: CheckpointSession>(session: &S, command: &Command) -> bool {
    let mut pages: HashMap<PageId, Option<PageContent>> = HashMap::new();
    let mut current = |page: PageId| -> Option<PageContent> {
        pages
            .entry(page)
            .or_insert_with(|| {
                read_pending_page_content_of(
                    session.document(),
                    page,
                    session.base(),
                    ImportedSources::none(),
                )
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

/// `command` with every target replaced by the item the live base holds for
/// it. A target that cannot be told apart is left as it was, and the edit
/// then fails validation instead of landing on the wrong item.
fn rebind<S: CheckpointSession>(session: &S, command: Command) -> Command {
    let mut pages: HashMap<PageId, Option<PageContent>> = HashMap::new();
    let mut content = |page: PageId| -> Option<PageContent> {
        pages
            .entry(page)
            .or_insert_with(|| {
                read_page_content_of(
                    session.document(),
                    page,
                    session.base(),
                    ImportedSources::none(),
                )
                .ok()
            })
            .clone()
    };
    map_targets(command, &mut |held| rebind_item(held, &mut content))
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
