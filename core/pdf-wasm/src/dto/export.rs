//! The values `Engine`'s image-export surface hands to JavaScript (B26 T-252).

use wasm_bindgen::prelude::*;

/// An open image export: its id, and how many pages its snapshot has — the
/// count an export is planned against, since the document may have changed
/// since the shell last looked.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct OpenedImageExport {
    id: u32,
    page_count: u32,
}

impl OpenedImageExport {
    pub(crate) fn new(id: u32, page_count: u32) -> Self {
        OpenedImageExport { id, page_count }
    }
}

#[wasm_bindgen]
impl OpenedImageExport {
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> u32 {
        self.id
    }

    #[wasm_bindgen(getter, js_name = pageCount)]
    pub fn page_count(&self) -> u32 {
        self.page_count
    }
}
