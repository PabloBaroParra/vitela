//! The value `Engine`'s forms surface hands to JavaScript (B26 T-242).
//!
//! Flat on purpose, like `AnnotationEntry`: one class for every kind, every
//! field present (empty, `false` or absent when the kind has none).
//! wasm-bindgen has no tagged unions and mistypes an `Option` in the `.d.ts`
//! (T-227), so `@vitela/core` decodes these by `kind` into its own typed
//! union and never trusts an absent field. Geometry is in the page's
//! unrotated PDF space, bottom-left origin.

use wasm_bindgen::prelude::*;

use crate::convert::annotation::rect_values;
use crate::convert::form::radio_options;
use crate::model::{FieldShape, FormFieldView};

/// One AcroForm field, as `Engine.formFields` lists it.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct FormFieldEntry {
    id: u32,
    page: u32,
    name: String,
    kind: &'static str,
    rect: Vec<f64>,
    existing: bool,
    multiline: bool,
    max_length: Option<u32>,
    options: Vec<String>,
    option_rects: Vec<f64>,
    editable: bool,
    text: String,
    checked: bool,
    selected: Option<String>,
}

impl From<FormFieldView> for FormFieldEntry {
    fn from(view: FormFieldView) -> Self {
        let mut entry = FormFieldEntry {
            id: view.id,
            page: view.page,
            name: view.name,
            kind: "",
            rect: rect_values(view.rect),
            existing: view.existing,
            multiline: false,
            max_length: None,
            options: Vec::new(),
            option_rects: Vec::new(),
            editable: false,
            text: String::new(),
            checked: false,
            selected: None,
        };
        match view.shape {
            FieldShape::Text {
                multiline,
                max_len,
                value,
            } => {
                entry.kind = "text";
                entry.multiline = multiline;
                entry.max_length = max_len;
                entry.text = value;
            }
            FieldShape::Checkbox { checked } => {
                entry.kind = "checkbox";
                entry.checked = checked;
            }
            FieldShape::Radio { options, selected } => {
                entry.kind = "radio";
                (entry.options, entry.option_rects) = radio_options(options);
                entry.selected = selected;
            }
            FieldShape::Dropdown {
                options,
                editable,
                selected,
            } => {
                entry.kind = "dropdown";
                entry.options = options;
                entry.editable = editable;
                entry.selected = selected;
            }
        }
        entry
    }
}

#[wasm_bindgen]
impl FormFieldEntry {
    /// What the fill constructors of `EditCommand` name it by.
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Position of its page now.
    #[wasm_bindgen(getter)]
    pub fn page(&self) -> u32 {
        self.page
    }

    /// Its fully qualified `/T`.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// `text`, `checkbox`, `radio` or `dropdown`.
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        self.kind.to_string()
    }

    /// The widget's `x, y, width, height` (a radio group's whole group).
    #[wasm_bindgen(getter)]
    pub fn rect(&self) -> Vec<f64> {
        self.rect.clone()
    }

    /// The opened file already had it, so pdfium paints it.
    #[wasm_bindgen(getter)]
    pub fn existing(&self) -> bool {
        self.existing
    }

    /// A text field that takes line breaks.
    #[wasm_bindgen(getter)]
    pub fn multiline(&self) -> bool {
        self.multiline
    }

    /// A text field's `/MaxLen`, in characters; absent when it has none.
    #[wasm_bindgen(getter, js_name = maxLength)]
    pub fn max_length(&self) -> Option<u32> {
        self.max_length
    }

    /// A dropdown's choices, or a radio group's export values (one per
    /// button); empty for the other kinds.
    #[wasm_bindgen(getter)]
    pub fn options(&self) -> Vec<String> {
        self.options.clone()
    }

    /// Each radio button's `x, y, width, height`, in `options` order; empty
    /// for the other kinds.
    #[wasm_bindgen(getter, js_name = optionRects)]
    pub fn option_rects(&self) -> Vec<f64> {
        self.option_rects.clone()
    }

    /// A dropdown that takes a value outside its `options`.
    #[wasm_bindgen(getter)]
    pub fn editable(&self) -> bool {
        self.editable
    }

    /// A text field's value; empty for the other kinds.
    #[wasm_bindgen(getter)]
    pub fn text(&self) -> String {
        self.text.clone()
    }

    /// Whether a checkbox is on; `false` for the other kinds.
    #[wasm_bindgen(getter)]
    pub fn checked(&self) -> bool {
        self.checked
    }

    /// The export value of the radio button on, or the dropdown's value;
    /// absent when nothing is chosen, and for the other kinds.
    #[wasm_bindgen(getter)]
    pub fn selected(&self) -> Option<String> {
        self.selected.clone()
    }
}
