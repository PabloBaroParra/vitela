//! The `Engine`'s selection and clipboard surface (B26 T-262): the edits
//! `applyEdit` records on several content items at once — move and remove a
//! selection, paste a clip — and the copy that hands a clip out.
//! `pdf-ffi`'s `move_content_selection`, `remove_content_selection`,
//! `copy_content_selection`, `cut_content_selection` and `paste_content`.
//!
//! The rules live in [`crate::model`]'s selection and clipboard modules,
//! over the core; this is the JavaScript boundary. Ids cross as numbers
//! (below 2^53); geometry is in PDF points (y up). The clip crosses as plain
//! values the shell holds on to: nothing here keeps it.

use pdf_document::{ContentClip, Rect};
use wasm_bindgen::prelude::*;

use super::content::item_id;
use super::edit::Edit;
use super::{js_error, EditCommand, Engine};
use crate::dto::ContentClipEntry;
use crate::error::{EngineError, ErrorKind};
use crate::model::{ContentMembers, SelectionEdit};

#[wasm_bindgen]
impl EditCommand {
    /// Shifts every member of a selection on page `page` — the runs, images
    /// and paths a content read handed out — by `dx, dy` PDF points (y up),
    /// as one undo step. A second move of the same selection amends its
    /// step; everything one pending paste added moves the paste itself.
    #[wasm_bindgen(js_name = moveContentSelection)]
    pub fn move_content_selection(
        page: u32,
        run_ids: Vec<f64>,
        image_ids: Vec<f64>,
        path_ids: Vec<f64>,
        dx: f64,
        dy: f64,
    ) -> Result<EditCommand, JsValue> {
        let members = members(run_ids, image_ids, path_ids).map_err(js_error)?;
        Ok(EditCommand(Edit::Selection(SelectionEdit::Move {
            page,
            members,
            dx,
            dy,
        })))
    }

    /// Takes every member of a selection on page `page` off it, as one undo
    /// step.
    #[wasm_bindgen(js_name = removeContentSelection)]
    pub fn remove_content_selection(
        page: u32,
        run_ids: Vec<f64>,
        image_ids: Vec<f64>,
        path_ids: Vec<f64>,
    ) -> Result<EditCommand, JsValue> {
        let members = members(run_ids, image_ids, path_ids).map_err(js_error)?;
        Ok(EditCommand(Edit::Selection(SelectionEdit::Remove {
            page,
            members,
        })))
    }

    /// Writes a clip `copyContentSelection` handed out — its `pdf`, `bbox`
    /// (`x, y, width, height`) and `fromEncrypted`, unchanged — onto page
    /// `page`, its box's top-left corner at `left, top` (PDF points), as one
    /// undo step.
    #[wasm_bindgen(js_name = pasteContent)]
    pub fn paste_content(
        page: u32,
        pdf: Vec<u8>,
        bbox: Vec<f64>,
        from_encrypted: bool,
        left: f64,
        top: f64,
    ) -> Result<EditCommand, JsValue> {
        let [x, y, width, height] = <[f64; 4]>::try_from(bbox).map_err(|_| {
            js_error(EngineError::new(
                ErrorKind::UnsupportedOperation,
                "a clip's box is x, y, width and height",
            ))
        })?;
        let clip = ContentClip {
            pdf,
            bbox: Rect {
                x,
                y,
                width,
                height,
            },
            from_encrypted,
        };
        Ok(EditCommand(Edit::Paste {
            page,
            clip,
            at: (left, top),
        }))
    }
}

#[wasm_bindgen]
impl Engine {
    /// Copies the members of a selection on page `page_index` into a clip,
    /// leaving the page as it is; members with an unsaved edit are copied as
    /// they read now. `forCut` asks the cut's permission (extraction and
    /// content editing, `EditNotPermitted` otherwise) instead of the copy's
    /// (`TextExtractionDenied`); the cut's removal is a
    /// `removeContentSelection` that follows. Then `PageIndexOutOfBounds`,
    /// or `UnsupportedOperation` for an empty selection or an id the page
    /// does not hold now.
    #[wasm_bindgen(js_name = copyContentSelection)]
    pub fn copy_content_selection(
        &self,
        doc: u32,
        page_index: u32,
        run_ids: Vec<f64>,
        image_ids: Vec<f64>,
        path_ids: Vec<f64>,
        for_cut: bool,
    ) -> Result<ContentClipEntry, JsValue> {
        let members = members(run_ids, image_ids, path_ids).map_err(js_error)?;
        self.document(doc)
            .and_then(|open| {
                open.model
                    .copy_content_selection(page_index, &members, for_cut)
            })
            .map(Into::into)
            .map_err(js_error)
    }
}

fn members(
    run_ids: Vec<f64>,
    image_ids: Vec<f64>,
    path_ids: Vec<f64>,
) -> Result<ContentMembers, EngineError> {
    let ids = |ids: Vec<f64>| ids.into_iter().map(item_id).collect::<Result<Vec<_>, _>>();
    Ok(ContentMembers {
        runs: ids(run_ids)?,
        images: ids(image_ids)?,
        paths: ids(path_ids)?,
    })
}
