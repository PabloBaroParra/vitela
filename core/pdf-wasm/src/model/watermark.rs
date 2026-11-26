//! Stamping a watermark — the same text or picture on many pages — over the
//! core (B26 T-256).
//!
//! Every rule is the core's, asked the way `pdf-ffi` asks it for
//! `FfiEditCommand::AddWatermark` and `AddImageWatermark`:
//!
//! - **The gate** is `pdf_save::check_edit` with the page-content class: the
//!   modify-contents permission (`/P` bit 4), then whether the full-rewrite
//!   writer a content edit forces can reproduce the document's encryption.
//!   It is asked before anything is built, so a refused stamp is refused
//!   even when its pages are also wrong.
//! - **The pages** cross as positions and are resolved to `PageId`s here,
//!   once: the stamp then stays on those pages through a later reorder.
//! - **The mark** is checked before it is recorded
//!   (`pdf_save::validate_content_command`, i.e. `pdf_edit::check_watermark`):
//!   a character Helvetica cannot show is `EncodingGap` and bytes that are
//!   not a PNG or JPEG `InvalidImage`, so nothing that can never be saved is
//!   queued. The look and the page list are the model's
//!   (`Watermark::can_apply_to`): blank text, a repeated page or an opacity
//!   nobody could see is `UnsupportedOperation`.
//! - **The look** the dialog starts from is `Watermark::diagonal` /
//!   `Watermark::picture` ([`watermark_defaults`]), as `pdf-ffi`'s
//!   `default_watermark`: no shell copies the constants.
//!
//! A stamp is page content: one undo step whatever the page count, shown by
//! the preview after a refresh, painted into each page's content stream by
//! the next save.

use pdf_document::{Command, PageId, Watermark, WatermarkLayer, WatermarkMark};
use pdf_save::{EditClass, ImportedSources};

use super::DocumentModel;
use crate::error::{EngineError, ErrorKind};

/// What a stamp paints.
#[derive(Debug, Clone, PartialEq)]
pub enum WatermarkStamp {
    /// A line of text in Helvetica, in the core's colour.
    Text { text: String, font_size: f64 },
    /// An encoded PNG or JPEG, fitted (aspect kept) inside the visible box
    /// shrunk by `scale`.
    Picture { bytes: Vec<u8>, scale: f64 },
}

/// One "Add watermark" action: the mark, its look and the pages (0-based
/// positions, as the document has them now).
#[derive(Debug, Clone, PartialEq)]
pub struct WatermarkRequest {
    pub pages: Vec<u32>,
    pub stamp: WatermarkStamp,
    /// `0.0` exclusive to `1.0` inclusive.
    pub opacity: f64,
    /// Counter-clockwise, as the reader sees the page.
    pub rotation_degrees: f64,
    /// Painted before the page's own content instead of after it.
    pub behind: bool,
}

/// The looks a watermark dialog starts from: the core's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WatermarkDefaults {
    pub font_size: f64,
    pub text_opacity: f64,
    pub text_rotation_degrees: f64,
    pub text_behind: bool,
    pub picture_scale: f64,
    pub picture_opacity: f64,
    pub picture_rotation_degrees: f64,
    pub picture_behind: bool,
}

/// The core's starting looks (`Watermark::diagonal` and `Watermark::picture`).
pub fn watermark_defaults() -> WatermarkDefaults {
    let text = Watermark::diagonal(Vec::new(), "");
    let picture = Watermark::picture(Vec::new(), Vec::new());
    let WatermarkMark::Text { font_size, .. } = text.mark else {
        unreachable!("Watermark::diagonal always builds a text mark");
    };
    let WatermarkMark::Image { scale, .. } = picture.mark else {
        unreachable!("Watermark::picture always builds an image mark");
    };
    WatermarkDefaults {
        font_size,
        text_opacity: text.opacity,
        text_rotation_degrees: text.rotation_degrees,
        text_behind: text.layer == WatermarkLayer::UnderContent,
        picture_scale: scale,
        picture_opacity: picture.opacity,
        picture_rotation_degrees: picture.rotation_degrees,
        picture_behind: picture.layer == WatermarkLayer::UnderContent,
    }
}

/// The gate's class: a stamp rewrites the pages' own content.
const CONTENT: EditClass = EditClass {
    annotation: false,
    document_assembly: false,
    structural_form: false,
    content: true,
    page_structure: false,
};

impl DocumentModel {
    /// Why no watermark can be stamped on this document, as the core's
    /// sentence, or `None` when one can. Cheap: it reads the permissions.
    pub fn watermark_refusal(&self) -> Option<String> {
        pdf_save::check_edit(self.document.security.as_ref(), CONTENT)
            .err()
            .map(|refusal| refusal.to_string())
    }

    /// Gates, checks and records `request` as one undo step. Nothing is
    /// recorded when any step refuses.
    pub fn add_watermark(&mut self, request: WatermarkRequest) -> Result<(), EngineError> {
        pdf_save::check_edit(self.document.security.as_ref(), CONTENT)?;
        let pages = request
            .pages
            .iter()
            .map(|&position| self.page_id_at(position))
            .collect::<Result<Vec<_>, _>>()?;
        let mark = match request.stamp {
            WatermarkStamp::Text { text, font_size } => WatermarkMark::Text {
                text,
                font_size,
                color: Watermark::DEFAULT_COLOR,
            },
            WatermarkStamp::Picture { bytes, scale } => WatermarkMark::Image { bytes, scale },
        };
        let command = Command::AddWatermark(Watermark {
            pages,
            mark,
            opacity: request.opacity,
            rotation_degrees: request.rotation_degrees,
            layer: if request.behind {
                WatermarkLayer::UnderContent
            } else {
                WatermarkLayer::OverContent
            },
        });
        let sources = self.imported.pairs();
        pdf_save::validate_content_command(
            &self.document,
            &self.base,
            ImportedSources::new(&sources),
            &command,
        )
        .map_err(mark_refusal)?;
        self.record(command)
    }

    fn page_id_at(&self, position: u32) -> Result<PageId, EngineError> {
        self.document
            .pages
            .get(position as usize)
            .map(|page| page.id)
            .ok_or_else(|| EngineError::page_out_of_bounds(position))
    }
}

/// A mark the core cannot paint, by the kinds `pdf-ffi` gives the same
/// failures.
fn mark_refusal(err: pdf_save::SaveError) -> EngineError {
    match err {
        pdf_save::SaveError::Edit(edit) => {
            let kind = match edit {
                pdf_edit::EditError::EncodingGap { .. } => ErrorKind::EncodingGap,
                pdf_edit::EditError::InvalidImage(_) => ErrorKind::InvalidImage,
                _ => ErrorKind::UnsupportedOperation,
            };
            EngineError::new(kind, edit.to_string())
        }
        other => EngineError::new(ErrorKind::UnsupportedOperation, other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{pdfs, PageEdit};
    use pdf_manip::OpenCredentials;

    fn open(bytes: &[u8]) -> DocumentModel {
        DocumentModel::open(bytes, &OpenCredentials::NoPassword).expect("opens")
    }

    fn text(pages: Vec<u32>, text: &str) -> WatermarkRequest {
        let look = watermark_defaults();
        WatermarkRequest {
            pages,
            stamp: WatermarkStamp::Text {
                text: text.to_string(),
                font_size: look.font_size,
            },
            opacity: look.text_opacity,
            rotation_degrees: look.text_rotation_degrees,
            behind: look.text_behind,
        }
    }

    fn picture(pages: Vec<u32>, bytes: &[u8]) -> WatermarkRequest {
        let look = watermark_defaults();
        WatermarkRequest {
            pages,
            stamp: WatermarkStamp::Picture {
                bytes: bytes.to_vec(),
                scale: look.picture_scale,
            },
            opacity: look.picture_opacity,
            rotation_degrees: look.picture_rotation_degrees,
            behind: look.picture_behind,
        }
    }

    /// Every page of `bytes`: the texts it paints and how many images.
    fn saved_pages(bytes: &[u8]) -> Vec<(Vec<String>, usize)> {
        let reloaded = lopdf::Document::load_mem(bytes).expect("output reloads");
        (0..reloaded.get_pages().len() as u32)
            .map(|index| {
                let content = pdf_edit::read_page_content(&reloaded, PageId(index)).unwrap();
                let texts = content.text_runs.into_iter().map(|run| run.text).collect();
                (texts, content.images.len())
            })
            .collect()
    }

    #[test]
    fn the_defaults_are_the_cores() {
        let look = watermark_defaults();
        assert_eq!(look.font_size, Watermark::DEFAULT_FONT_SIZE);
        assert_eq!(look.text_opacity, Watermark::DEFAULT_OPACITY);
        assert_eq!(look.text_rotation_degrees, 45.0);
        assert!(!look.text_behind);
        assert_eq!(look.picture_scale, Watermark::DEFAULT_IMAGE_SCALE);
        assert_eq!(look.picture_rotation_degrees, 0.0);
        assert!(look.picture_behind);
    }

    #[test]
    fn a_text_stamp_on_every_page_is_one_undo_step_written_by_the_save() {
        let mut model = open(&pdfs::text_document(&["one", "two"]));
        assert_eq!(model.watermark_refusal(), None);
        assert_eq!(model.paint_revision(), 0);
        model.add_watermark(text(vec![0, 1], "DRAFT")).unwrap();
        assert!(model.can_undo());
        assert!(model.preview_stale());
        let stamped = model.paint_revision();
        assert_ne!(stamped, 0, "the pages repaint in place");

        let saved = saved_pages(&model.save_to_bytes(false).unwrap());
        assert_eq!(saved.len(), 2);
        for (texts, _) in &saved {
            assert!(texts.contains(&"DRAFT".to_string()), "{texts:?}");
        }

        assert!(model.undo());
        assert!(!model.can_undo());
        assert_eq!(model.paint_revision(), 0);
        let saved = saved_pages(&model.save_to_bytes(false).unwrap());
        assert!(saved
            .iter()
            .all(|(texts, _)| !texts.contains(&"DRAFT".to_string())));
        assert!(model.redo());
        assert_eq!(model.paint_revision(), stamped);
        let saved = saved_pages(&model.save_to_bytes(false).unwrap());
        assert!(saved[1].0.contains(&"DRAFT".to_string()));
    }

    #[test]
    fn a_stamp_stays_on_its_pages_through_a_later_move() {
        let mut model = open(&pdfs::text_document(&["one", "two"]));
        model.add_watermark(text(vec![1], "COPY")).unwrap();
        let stamped = model.paint_revision();
        model
            .apply_edit(PageEdit::Move {
                from: 1,
                count: 1,
                to: 0,
            })
            .unwrap();
        assert_eq!(
            model.paint_revision(),
            stamped,
            "a move repaints nothing in place"
        );
        let saved = saved_pages(&model.save_to_bytes(false).unwrap());
        assert!(saved[0].0.contains(&"COPY".to_string()), "{saved:?}");
        assert!(!saved[1].0.contains(&"COPY".to_string()), "{saved:?}");
    }

    #[test]
    fn a_picture_stamp_paints_an_image_on_its_pages() {
        let mut model = open(&pdfs::text_document(&["one", "two"]));
        model
            .add_watermark(picture(vec![0], pdfs::PNG_1X1))
            .unwrap();
        let saved = saved_pages(&model.save_to_bytes(false).unwrap());
        assert_eq!(saved[0].1, 1);
        assert_eq!(saved[1].1, 0);
    }

    #[test]
    fn a_mark_the_core_cannot_paint_records_nothing() {
        let mut model = open(&pdfs::text_document(&["one"]));
        let err = model.add_watermark(text(vec![0], "日本")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::EncodingGap);
        let err = model
            .add_watermark(picture(vec![0], b"not a picture"))
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidImage);
        let err = model.add_watermark(text(vec![0], "   ")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsupportedOperation);
        let err = model.add_watermark(text(vec![0, 0], "DRAFT")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsupportedOperation);
        let err = model.add_watermark(text(vec![], "DRAFT")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsupportedOperation);
        let err = model.add_watermark(text(vec![1], "DRAFT")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::PageIndexOutOfBounds);
        assert!(!model.can_undo());
    }

    #[test]
    fn a_document_that_withholds_modification_refuses_before_the_pages_are_read() {
        let mut model = DocumentModel::open(
            &pdfs::restricted_document(),
            &OpenCredentials::Password(pdfs::USER_PASSWORD.into()),
        )
        .unwrap();
        let refusal = model.watermark_refusal().expect("refused");
        assert_eq!(refusal, "content editing is not permitted");
        // Page 9 does not exist: the permission is still what is reported.
        let err = model.add_watermark(text(vec![9], "DRAFT")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::EditNotPermitted);
        assert_eq!(err.message, refusal);
        assert!(!model.can_undo());
    }
}
