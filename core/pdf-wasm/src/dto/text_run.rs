//! The values `Engine`'s text-run editing surface hands to JavaScript
//! (B26 T-259): how a run looks now, and the font program its font embeds.

use wasm_bindgen::prelude::*;

use pdf_edit::{FontProgram, FontProgramFormat, RunLook};

/// How a run looks with every pending edit in place, and which styles can
/// come off it, as `Engine.textRunLook` reads it.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct TextRunLookEntry {
    look: RunLook,
}

impl From<RunLook> for TextRunLookEntry {
    fn from(look: RunLook) -> Self {
        TextRunLookEntry { look }
    }
}

#[wasm_bindgen]
impl TextRunLookEntry {
    #[wasm_bindgen(getter)]
    pub fn bold(&self) -> bool {
        self.look.look.bold
    }

    #[wasm_bindgen(getter)]
    pub fn italic(&self) -> bool {
        self.look.look.italic
    }

    #[wasm_bindgen(getter)]
    pub fn underline(&self) -> bool {
        self.look.look.underline
    }

    #[wasm_bindgen(getter)]
    pub fn strike(&self) -> bool {
        self.look.look.strike
    }

    /// `false` only for a run painted in a bold font whose page has no
    /// regular sibling: un-bolding is never faked.
    #[wasm_bindgen(getter, js_name = canRemoveBold)]
    pub fn can_remove_bold(&self) -> bool {
        self.look.can_remove_bold
    }

    /// The same, for italic.
    #[wasm_bindgen(getter, js_name = canRemoveItalic)]
    pub fn can_remove_italic(&self) -> bool {
        self.look.can_remove_italic
    }
}

/// A font program embedded in the file, decoded, as
/// `Engine.textRunFontProgram` reads it.
#[wasm_bindgen]
pub struct FontProgramEntry {
    program: FontProgram,
}

impl From<FontProgram> for FontProgramEntry {
    fn from(program: FontProgram) -> Self {
        FontProgramEntry { program }
    }
}

#[wasm_bindgen]
impl FontProgramEntry {
    /// `trueType`, `openType`, `cff` (a bare CFF table) or `type1`: a
    /// browser's `FontFace` takes the first two.
    #[wasm_bindgen(getter)]
    pub fn format(&self) -> String {
        format_name(self.program.format).to_string()
    }

    /// The program's bytes, moved out (empty afterwards).
    #[wasm_bindgen(js_name = takeBytes)]
    pub fn take_bytes(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.program.bytes)
    }
}

fn format_name(format: FontProgramFormat) -> &'static str {
    match format {
        FontProgramFormat::TrueType => "trueType",
        FontProgramFormat::OpenType => "openType",
        FontProgramFormat::Cff => "cff",
        FontProgramFormat::Type1 => "type1",
    }
}
