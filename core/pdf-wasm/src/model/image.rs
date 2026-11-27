//! Editing the images a page paints (B26 T-260): move, resize, remove and
//! replace one image, insert a new one, and the question a shell asks before
//! it opens its file picker for a replacement
//! (`pdf_save::check_image_replacement`).
//!
//! Every rule is the core's. An image is named by its page position and the
//! id [`DocumentModel::read_page_content`] handed out; it is resolved here
//! against the pending read — the image exactly as the reader sees it, which
//! is what `pdf_save::record_content_command` folds against — and the edit
//! is the core command a native shell records, through
//! [`DocumentModel::record_content`]: one undo step, a second move or resize
//! of the same image amends its queued step, and a checkpoint is taken when
//! the queue refuses it (an image a pending insert added, an image with a
//! queued replacement).
//!
//! An inserted image's size is the core's placement policy
//! (`pdf_annotate::stamp_placement`, as every native shell asks it): its own
//! proportions, the longest side [`pdf_annotate::DEFAULT_STAMP_MAX_SIDE_PT`],
//! its top-left corner at the point given. Its `/XObject` name is picked
//! here, as the Android adapter picks one: random, so a name a removed
//! image left in the page's resources after a save is not hit again (the
//! core refuses a taken name rather than reusing it).

use pdf_document::{Command, ContentItemId, ImageItem, ImageSource, PageId, Rect};
use pdf_save::{IdGenerator, ImportedSources, RandomIdGenerator};

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// One edit of the images on one page. Positions are 0-based; image ids are
/// the ones the page's last content read handed out. Geometry is in PDF
/// points (y up).
#[derive(Debug, Clone, PartialEq)]
pub enum ImageEdit {
    /// Shifts the image by `dx, dy`, its size kept.
    Move {
        page: u32,
        image: u64,
        dx: f64,
        dy: f64,
    },
    /// Paints the image into `to`.
    Resize { page: u32, image: u64, to: Rect },
    /// Takes the image off the page.
    Remove { page: u32, image: u64 },
    /// Shows `bytes` (an encoded PNG or JPEG) in place of the image's
    /// picture, in its place and size.
    Replace {
        page: u32,
        image: u64,
        bytes: Vec<u8>,
    },
    /// Paints `bytes` (an encoded PNG or JPEG) as a new image with its
    /// top-left corner at `at`, at the size the core's placement gives it.
    Insert {
        page: u32,
        at: (f64, f64),
        bytes: Vec<u8>,
    },
}

/// The prefix of the `/XObject` name an inserted image is registered under.
const INSERTED_NAME_PREFIX: &str = "VitelaImage";

impl DocumentModel {
    /// Records `edit` as one undo step, or folds it into the queued step of
    /// the same image. An edit that changes nothing (a zero move, a resize
    /// to the box it has) records nothing.
    ///
    /// Refused, in this order: `EditNotPermitted` (or `UnsupportedOperation`
    /// for an encryption the rewrite could not keep) when the document's
    /// content cannot be edited; `TextExtractionDenied` when its content
    /// cannot be read; `PageIndexOutOfBounds`; `UnsupportedOperation` for an
    /// image id the page does not hold now, a distance or box that is not
    /// finite, or a box with no area; `InvalidImage` for bytes that do not
    /// decode; then whatever the core's write refuses.
    pub fn apply_image_edit(&mut self, edit: ImageEdit) -> Result<(), EngineError> {
        pdf_save::ensure_content_editing(self.document.security.as_ref())?;
        self.ensure_text_extraction_allowed()?;
        let Some(command) = self.image_command(edit)? else {
            return Ok(());
        };
        self.record_content(command)
    }

    /// Whether image `image_id` on page `page_index` could be replaced now,
    /// asked before a shell opens its file picker
    /// (`pdf_save::check_image_replacement`): `EditNotPermitted` for a
    /// document that forbids content changes, `UnsupportedOperation` for an
    /// image with an unsaved edit in a session that may not checkpoint, an
    /// image the page does not paint now, or a picture the core could not
    /// put back on undo. Same read gate as [`Self::read_page_content`].
    pub fn check_image_replacement(
        &self,
        page_index: u32,
        image_id: u64,
    ) -> Result<(), EngineError> {
        // Asked before the image is resolved, as `pdf-ffi` asks it: a
        // document that forbids the change says so even when the id is stale.
        if !self.content_editing_allowed() {
            return Err(pdf_save::ContentEditRefusal::ImageReplacement.into());
        }
        self.ensure_text_extraction_allowed()?;
        let item = self.image_at(page_index, image_id)?;
        let sources = self.imported.pairs();
        pdf_save::check_image_replacement::<_, EngineError>(
            self,
            ImportedSources::new(&sources),
            &item,
        )
    }

    /// The core command for `edit`, or `None` when it would change nothing.
    fn image_command(&self, edit: ImageEdit) -> Result<Option<Command>, EngineError> {
        Ok(match edit {
            ImageEdit::Move {
                page,
                image,
                dx,
                dy,
            } => {
                if !(dx.is_finite() && dy.is_finite()) {
                    return Err(unsupported("a move is a finite distance"));
                }
                let item = self.image_at(page, image)?;
                if dx == 0.0 && dy == 0.0 {
                    return Ok(None);
                }
                let to = Rect {
                    x: item.bbox.x + dx,
                    y: item.bbox.y + dy,
                    ..item.bbox
                };
                Some(Command::MoveImage { item, to })
            }
            ImageEdit::Resize { page, image, to } => {
                if ![to.x, to.y, to.width, to.height]
                    .iter()
                    .all(|value| value.is_finite())
                    || to.width <= 0.0
                    || to.height <= 0.0
                {
                    return Err(unsupported(
                        "an image is resized to a finite box with an area",
                    ));
                }
                let item = self.image_at(page, image)?;
                if item.bbox == to {
                    return Ok(None);
                }
                Some(Command::ResizeImage { item, to })
            }
            ImageEdit::Remove { page, image } => Some(Command::RemoveImage {
                item: self.image_at(page, image)?,
                source: None,
            }),
            ImageEdit::Replace { page, image, bytes } => {
                self.check_image_replacement(page, image)?;
                let item = self.image_at(page, image)?;
                // A file that does not decode is refused before anything is queued.
                decodes(&bytes)?;
                Some(Command::ReplaceImageSource { item, after: bytes })
            }
            ImageEdit::Insert { page, at, bytes } => {
                if !(at.0.is_finite() && at.1.is_finite()) {
                    return Err(unsupported("an image is inserted at a finite point"));
                }
                let page_id = self.image_page_id(page)?;
                let bbox = super::stamp_placement(&bytes, at)?;
                let item = ImageItem {
                    id: ContentItemId(0),
                    page: page_id,
                    bbox,
                    source: ImageSource::Resource(self.unused_image_name(page_id)?),
                };
                Some(Command::InsertImage {
                    item,
                    source: Some(bytes),
                })
            }
        })
    }

    /// An `/XObject` name no image on `page` paints and no queued insert or
    /// removal claims, random so it misses what a removed image left behind.
    fn unused_image_name(&self, page: PageId) -> Result<String, EngineError> {
        let sources = self.imported.pairs();
        let content = pdf_save::read_pending_page_content_of(
            &self.document,
            page,
            &self.base,
            ImportedSources::new(&sources),
        )
        .map_err(|err| EngineError::new(ErrorKind::InvalidDocument, err.to_string()))?;
        let claimed = |name: &str| {
            content
                .images
                .iter()
                .any(|image| image.resource_xobject_name() == Some(name))
                || self.document.pending_edits.entries().iter().any(|command| {
                    matches!(
                        command,
                        Command::InsertImage { item, .. } | Command::RemoveImage { item, .. }
                            if item.resource_xobject_name() == Some(name)
                    )
                })
        };
        let ids = RandomIdGenerator::new();
        loop {
            let suffix: String = ids
                .next_id()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let name = format!("{INSERTED_NAME_PREFIX}{suffix}");
            if !claimed(&name) {
                return Ok(name);
            }
        }
    }

    /// Image `image_id` on page `page_index` as the reader sees it now.
    fn image_at(&self, page_index: u32, image_id: u64) -> Result<ImageItem, EngineError> {
        let page = self.image_page_id(page_index)?;
        let sources = self.imported.pairs();
        pdf_save::read_pending_page_content_of(
            &self.document,
            page,
            &self.base,
            ImportedSources::new(&sources),
        )
        .map_err(|err| EngineError::new(ErrorKind::InvalidDocument, err.to_string()))?
        .images
        .into_iter()
        .find(|image| image.id.0 == image_id)
        .ok_or_else(|| {
            unsupported("the page has no image with that id now: read its content again")
        })
    }

    fn image_page_id(&self, page_index: u32) -> Result<PageId, EngineError> {
        self.document
            .pages
            .get(page_index as usize)
            .map(|page| page.id)
            .ok_or_else(|| EngineError::page_out_of_bounds(page_index))
    }
}

/// Refuses, from its header alone, bytes that are not a PNG or JPEG of a
/// size the core decodes (`pdf_edit::probe_image`, as the placement reads
/// them). The file is decoded whole once, by the record's validation
/// (`pdf_save::record_content_command` applies the command before it queues
/// it), which refuses one whose samples are broken.
fn decodes(bytes: &[u8]) -> Result<(), EngineError> {
    pdf_edit::probe_image(bytes)?;
    Ok(())
}

fn unsupported(message: &str) -> EngineError {
    EngineError::new(ErrorKind::UnsupportedOperation, message)
}

#[cfg(test)]
mod tests;
