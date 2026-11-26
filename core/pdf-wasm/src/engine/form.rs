//! The `Engine`'s forms surface (B26 T-242): the fill `EditCommand`s
//! `applyEdit` records, the field list a forms panel reads, and the fill
//! permission.
//!
//! The rules live in [`crate::model`]'s form module, over the core; this is
//! the JavaScript boundary. One constructor per kind of value, since a
//! field's kind decides which one it takes: text for a text field, checked
//! for a checkbox, a choice for a radio group or a dropdown. Unlike an
//! annotation edit, a fill makes `previewStale` true: pdfium paints the
//! field, so the shell refreshes the preview before rendering again.

use pdf_document::FieldValue;
use wasm_bindgen::prelude::*;

use super::edit::Edit;
use super::{js_error, EditCommand, Engine};
use crate::dto::FormFieldEntry;
use crate::model::FieldFill;

fn fill(id: u32, value: FieldValue) -> EditCommand {
    EditCommand(Edit::Form(FieldFill { id, value }))
}

#[wasm_bindgen]
impl EditCommand {
    /// Types `text` into text field `id`. `UnsupportedOperation` on apply
    /// when the field is not a text field or the text is past its
    /// `maxLength`.
    #[wasm_bindgen(js_name = setFieldText)]
    pub fn set_field_text(id: u32, text: String) -> Self {
        fill(id, FieldValue::Text(text))
    }

    /// Turns checkbox `id` on or off. `UnsupportedOperation` on apply when
    /// the field is not a checkbox.
    #[wasm_bindgen(js_name = setFieldChecked)]
    pub fn set_field_checked(id: u32, checked: bool) -> Self {
        fill(id, FieldValue::Checked(checked))
    }

    /// Chooses `option` in radio group or dropdown `id` (an export value for
    /// a radio group), or nothing when it is absent. `UnsupportedOperation`
    /// on apply for a choice the field does not offer (an editable dropdown
    /// takes any) or a field that is neither.
    #[wasm_bindgen(js_name = setFieldChoice)]
    pub fn set_field_choice(id: u32, option: Option<String>) -> Self {
        fill(id, FieldValue::Choice(option))
    }
}

#[wasm_bindgen]
impl Engine {
    /// Every AcroForm field the document holds, in `/Fields` order, each
    /// with the position its page is at now — the opened file's fields
    /// included. Changes with every fill, undo and redo. Read-only: answers
    /// however restricted the document is.
    #[wasm_bindgen(js_name = formFields)]
    pub fn form_fields(&self, doc: u32) -> Result<Vec<FormFieldEntry>, JsValue> {
        self.document(doc)
            .map(|open| {
                open.model
                    .form_fields()
                    .into_iter()
                    .map(Into::into)
                    .collect()
            })
            .map_err(js_error)
    }

    /// Whether the document lets its fields be filled in — what a fill
    /// `applyEdit` refuses with `EditNotPermitted`.
    #[wasm_bindgen(js_name = formFillAllowed)]
    pub fn form_fill_allowed(&self, doc: u32) -> Result<bool, JsValue> {
        self.document(doc)
            .map(|open| open.model.form_fill_allowed())
            .map_err(js_error)
    }
}
