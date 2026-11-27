//! The questions every boundary asks around a page-content edit, and how it
//! records one through a checkpointing session.
//!
//! This used to live only in `pdf-ffi` (`document.rs`, `document/checkpoint.rs`,
//! `text_blocks.rs`, `content_clipboard.rs`), so a boundary that does not
//! cross UniFFI (`pdf-wasm`) would have had to copy it. None of it needs
//! anything a boundary owns: the document's [`SecurityContext`], its pending
//! queue, and a [`CheckpointSession`].
//!
//! A boundary keeps its own error type: every refusal here is a
//! [`ContentEditRefusal`], whose `Display` is the reader-facing message, and
//! the functions that can also fail on a read are generic over an error that
//! converts from each of their failures.

use pdf_document::{Command, ContentItemId, Document, ImageItem, PageId, SecurityContext, TextRun};

use crate::bridge::ImportedSources;
use crate::checkpoint::{self, CheckpointSession};
use crate::error::SaveError;
use crate::origin::{page_backing, PageBacking};
use crate::rewrite::full_rewrite_blocker;

/// Why a page-content edit, or a question asked before one, was refused. Its
/// `Display` is the reader-facing message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentEditRefusal {
    /// The document withholds the modify-contents permission, or its
    /// encryption could not survive the full rewrite a content edit forces
    /// ([`content_editing_allowed`]).
    ContentEditing,
    /// A cut needs both the copy and the content-edit permission
    /// ([`content_cut_allowed`]).
    Cut,
    /// [`ContentEditRefusal::ContentEditing`], asked before a replacement
    /// picture is chosen ([`check_image_replacement`]).
    ImageReplacement,
    /// The image already has an unsaved edit, in a session that may not
    /// checkpoint.
    ImagePendingEdit,
    /// The page the image is checked against has no original content.
    NoOriginalImage,
    /// The edit names an inline image whose queued replacement no save could
    /// replay it after, in a session that may not checkpoint
    /// ([`record_content`]).
    ReplacedInlineImage,
}

impl std::fmt::Display for ContentEditRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ContentEditRefusal::ContentEditing => "content editing is not permitted",
            ContentEditRefusal::Cut => "cutting content is not permitted",
            ContentEditRefusal::ImageReplacement => "image replacement is not permitted",
            ContentEditRefusal::ImagePendingEdit => {
                "save before replacing an image with a pending edit"
            }
            ContentEditRefusal::NoOriginalImage => "the page has no original image",
            ContentEditRefusal::ReplacedInlineImage => {
                "save before editing an image whose replacement is not saved yet"
            }
        })
    }
}

impl std::error::Error for ContentEditRefusal {}

/// Whether a document opened with `security` lets its page content be
/// rewritten — the modify-contents bit, gated like the annotate one but
/// separately: a document may grant either without the other.
///
/// Two questions, deliberately answered as one: whether the document permits
/// the change, and whether the result could ever be written. Every page-
/// content edit forces a full rewrite (Batch 21 decision 5), and an encrypted
/// document opened with only one of its two passwords cannot be re-encrypted
/// — so on such a document a content edit is not a change to be saved later,
/// it is one that can never be saved at all. A shell that offered it would
/// collect work it has no way to keep.
///
/// The rewrite half is [`full_rewrite_blocker`], the same function the save
/// encoder consults, so an AES-256 document the save would refuse is refused
/// here too.
pub fn content_editing_allowed(security: Option<&SecurityContext>) -> bool {
    pdf_manip::content_editing_is_allowed(security) && full_rewrite_blocker(security).is_none()
}

/// [`content_editing_allowed`] as a refusal: what a content edit that does
/// not go through [`crate::check_edit`] — a block or selection batch, a
/// paste — asks first.
pub fn ensure_content_editing(
    security: Option<&SecurityContext>,
) -> Result<(), ContentEditRefusal> {
    if content_editing_allowed(security) {
        Ok(())
    } else {
        Err(ContentEditRefusal::ContentEditing)
    }
}

/// Whether a document opened with `security` lets a selection be cut: a cut
/// is a copy and a removal, so it needs the text-extraction permission the
/// copy asks for and the content-edit one the removal does.
pub fn content_cut_allowed(security: Option<&SecurityContext>) -> bool {
    pdf_manip::text_extraction_is_allowed(security) && content_editing_allowed(security)
}

/// Records the page-content edit `command` through `apply`, checkpointing
/// once when `apply` refuses it and the session may take one
/// ([`checkpoint::record`]).
///
/// An edit of an inline image whose replacement is still queued is accepted
/// by the base but could not be replayed by a save: it is written into a new
/// base first ([`checkpoint::record_after_checkpoint`]), or refused with
/// [`ContentEditRefusal::ReplacedInlineImage`] where none may be taken.
///
/// `apply` is the recording path for this kind of edit — a single edit's or a
/// batch's — and must leave the session untouched when it refuses.
pub fn record_content<S: CheckpointSession, E: From<ContentEditRefusal>>(
    session: &mut S,
    command: Command,
    apply: impl FnMut(&mut S, Command) -> Result<(), E>,
) -> Result<(), E> {
    if checkpoint::edits_a_replaced_inline_image(session, &command) {
        let refusal = ContentEditRefusal::ReplacedInlineImage.into();
        return checkpoint::record_after_checkpoint(session, command, refusal, apply);
    }
    checkpoint::record(session, command, apply)
}

/// Whether `item` could be replaced now, asked before a shell opens its file
/// picker. Refuses a document that forbids content changes, an image that
/// already has an unsaved edit in a session that may not checkpoint, and an
/// item the page no longer paints. Every encoding is replaceable: undo
/// restores the original exactly, so nothing about the old picture is read.
///
/// An image with an unsaved edit, or one a pending edit added, in a session
/// that may checkpoint is accepted without reading the file: the file does
/// not hold it as the reader sees it, and recording the replacement writes
/// the queue into a new base and validates it against that.
pub fn check_image_replacement<'a, S, E>(
    session: &'a S,
    sources: ImportedSources<'_, 'a>,
    item: &ImageItem,
) -> Result<(), E>
where
    S: CheckpointSession,
    E: From<ContentEditRefusal> + From<SaveError> + From<pdf_edit::EditError>,
{
    let document = session.document();
    if !content_editing_allowed(document.security.as_ref()) {
        return Err(ContentEditRefusal::ImageReplacement.into());
    }
    // An image only a pending edit added — a paste — is not in the file to
    // be checked against; a checkpoint writes it there first.
    if pdf_edit::pending_log_index(item.id).is_some() && checkpoint::may_checkpoint(session) {
        return Ok(());
    }
    if image_has_pending_edit(document, item) {
        if checkpoint::may_checkpoint(session) {
            return Ok(());
        }
        return Err(ContentEditRefusal::ImagePendingEdit.into());
    }
    match page_backing(document, item.page, session.base(), sources)? {
        PageBacking::Object { document, object } => {
            pdf_edit::check_image_source(document.as_lopdf(), object, item).map_err(E::from)
        }
        PageBacking::Empty => Err(ContentEditRefusal::NoOriginalImage.into()),
    }
}

/// Whether a queued edit removes, moves, resizes or replaces `item`.
fn image_has_pending_edit(document: &Document, item: &ImageItem) -> bool {
    document.pending_edits.entries().iter().any(|command| {
        // `InsertImage` is left out on purpose: shells record an insert with
        // `id: 0`, which is also the id the page's first original image reads
        // with. The inserted image itself is still refused by
        // [`check_image_replacement`] — the page it is checked against does
        // not paint it yet.
        let target = match command {
            Command::RemoveImage { item, .. }
            | Command::MoveImage { item, .. }
            | Command::ResizeImage { item, .. }
            | Command::ReplaceImageSource { item, .. } => item,
            _ => return false,
        };
        target.page == item.page && target.id == item.id
    })
}

/// Whether `run`'s text decoded fully: every code its font paints read
/// back as a character. A code the font gives no character for — no usable
/// `/ToUnicode`, an encoding this build cannot map — reads as U+FFFD, and a
/// run holding one cannot be retyped: the editor would not know what it is
/// replacing, and the encoder refuses the U+FFFD it would be handed back
/// (`pdf_edit::text_run_bbox`'s `EncodingGap`). A shell shows such a run but
/// does not offer to retype it.
pub fn text_run_text_is_decoded(run: &TextRun) -> bool {
    !run.text.contains(char::REPLACEMENT_CHARACTER)
}

/// Whether the existing run `id` on `page` has an unsaved movement. Retyping
/// such a run is no longer refused when the session can checkpoint; a shell
/// that still gates on this keeps the old save-and-reopen rule for sessions
/// that cannot.
pub fn text_run_has_pending_move(document: &Document, page: PageId, id: ContentItemId) -> bool {
    document.pending_edits.entries().iter().any(|command| {
        matches!(command, Command::MoveTextRun { item, .. }
            if item.page == page && item.id == id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{
        Credential, EditLog, EncryptionCredentials, FontKind, ImageSource, Permissions, Rect,
        SecurityHandler, TextRun,
    };

    const MODIFY_CONTENTS: u32 = 1 << 3;
    const COPY: u32 = 1 << 4;

    /// An RC4 document opened with its user password, granting
    /// `permissions`; `owner_too` adds the owner password a full rewrite
    /// needs to re-encrypt it.
    fn rc4(permissions: u32, owner_too: bool) -> SecurityContext {
        SecurityContext {
            handler: SecurityHandler::Rc4_128,
            credential: Credential::User,
            credentials: if owner_too {
                EncryptionCredentials {
                    user_password: Some("user".to_string()),
                    owner_password: Some("owner".to_string()),
                }
            } else {
                EncryptionCredentials::default()
            },
            permissions: Permissions(permissions),
        }
    }

    #[test]
    fn a_run_with_an_undecoded_code_is_not_retypable() {
        let run = |text: &str| TextRun {
            id: ContentItemId(0),
            page: PageId(0),
            bbox: Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::EmbeddedSimple,
            text: text.to_string(),
        };
        assert!(text_run_text_is_decoded(&run("Hello")));
        assert!(text_run_text_is_decoded(&run("")));
        assert!(!text_run_text_is_decoded(&run("He\u{FFFD}lo")));
    }

    #[test]
    fn an_unencrypted_document_allows_content_editing_and_cutting() {
        assert!(content_editing_allowed(None));
        assert!(content_cut_allowed(None));
        assert_eq!(ensure_content_editing(None), Ok(()));
    }

    #[test]
    fn content_editing_needs_the_modify_contents_bit() {
        let security = rc4(COPY, true);
        assert!(!content_editing_allowed(Some(&security)));
        assert!(!content_cut_allowed(Some(&security)));
        assert_eq!(
            ensure_content_editing(Some(&security)),
            Err(ContentEditRefusal::ContentEditing)
        );
    }

    #[test]
    fn a_cut_needs_the_copy_bit_on_top() {
        let security = rc4(MODIFY_CONTENTS, true);
        assert!(content_editing_allowed(Some(&security)));
        assert!(!content_cut_allowed(Some(&security)));
        assert!(content_cut_allowed(Some(&rc4(
            MODIFY_CONTENTS | COPY,
            true
        ))));
    }

    /// Permitted, but opened with one password only: a full rewrite could
    /// not re-encrypt it, so no content edit could ever be saved.
    #[test]
    fn content_editing_needs_a_rewrite_that_keeps_the_encryption() {
        let security = rc4(MODIFY_CONTENTS | COPY, false);
        assert!(full_rewrite_blocker(Some(&security)).is_some());
        assert!(!content_editing_allowed(Some(&security)));
        assert!(!content_cut_allowed(Some(&security)));
        assert!(content_editing_allowed(Some(&rc4(
            MODIFY_CONTENTS | COPY,
            true
        ))));
    }

    #[test]
    fn the_refusal_messages_are_the_ones_the_boundaries_always_showed() {
        assert_eq!(
            ContentEditRefusal::ReplacedInlineImage.to_string(),
            "save before editing an image whose replacement is not saved yet"
        );
        assert_eq!(
            ContentEditRefusal::ImagePendingEdit.to_string(),
            "save before replacing an image with a pending edit"
        );
        assert_eq!(
            ContentEditRefusal::NoOriginalImage.to_string(),
            "the page has no original image"
        );
        assert_eq!(
            ContentEditRefusal::ImageReplacement.to_string(),
            "image replacement is not permitted"
        );
        assert_eq!(
            ContentEditRefusal::Cut.to_string(),
            "cutting content is not permitted"
        );
        assert_eq!(
            ContentEditRefusal::ContentEditing.to_string(),
            "content editing is not permitted"
        );
    }

    fn run(page: u32, id: u64) -> TextRun {
        TextRun {
            id: ContentItemId(id),
            page: PageId(page),
            bbox: Rect {
                x: 10.0,
                y: 10.0,
                width: 10.0,
                height: 10.0,
            },
            resource_font_name: "F1".to_string(),
            font_kind: FontKind::Standard14,
            text: "x".to_string(),
        }
    }

    fn image(page: u32, id: u64) -> ImageItem {
        ImageItem {
            id: ContentItemId(id),
            page: PageId(page),
            bbox: Rect {
                x: 10.0,
                y: 10.0,
                width: 10.0,
                height: 10.0,
            },
            source: ImageSource::Inline,
        }
    }

    fn with_pending(command: Command) -> Document {
        let mut document = Document::default();
        let mut log = EditLog::new();
        assert!(log.apply(&mut document, command), "the edit records");
        document.pending_edits = log;
        document
    }

    #[test]
    fn a_pending_move_is_found_on_its_own_run_only() {
        let document = with_pending(Command::MoveTextRun {
            item: run(1, 4),
            to: run(1, 4).bbox,
        });
        assert!(text_run_has_pending_move(
            &document,
            PageId(1),
            ContentItemId(4)
        ));
        assert!(!text_run_has_pending_move(
            &document,
            PageId(1),
            ContentItemId(5)
        ));
        assert!(!text_run_has_pending_move(
            &document,
            PageId(2),
            ContentItemId(4)
        ));
    }

    #[test]
    fn an_image_edit_is_pending_but_an_insert_is_not() {
        let moved = with_pending(Command::MoveImage {
            item: image(0, 0),
            to: image(0, 0).bbox,
        });
        assert!(image_has_pending_edit(&moved, &image(0, 0)));
        assert!(!image_has_pending_edit(&moved, &image(0, 1)));
        assert!(!image_has_pending_edit(&moved, &image(1, 0)));
        assert!(!image_has_pending_edit(&Document::default(), &image(0, 0)));
    }
}
