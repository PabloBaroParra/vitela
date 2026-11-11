//! Batch 26 phase-0 spike: the Vitela core inside a browser.
//!
//! Two independent halves, both called from a Web Worker (see `www/`):
//! - [`core_page_count`]: the lopdf-backed core (`pdf-manip`) on wasm32.
//! - [`render_page_rgba`]: pdfium (a separate Emscripten module, bound at
//!   runtime by pdfium-render's WASM bindings) rendering one page to RGBA.

use std::cell::OnceCell;

use pdfium_render::prelude::*;
use wasm_bindgen::prelude::*;

// No re-export needed: pdfium-render's own `#[wasm_bindgen]`
// `initialize_pdfium_render(pdfiumModule, rustModule, debug)` is linked into
// this cdylib and appears in the generated JS glue.

/// Opens `bytes` with the real core (`pdf_manip::open_document_from_bytes`)
/// and returns its page count.
#[wasm_bindgen]
pub fn core_page_count(bytes: &[u8]) -> Result<u32, JsError> {
    let (doc, _security) = pdf_manip::open_document_from_bytes(bytes, None)
        .map_err(|e| JsError::new(&format!("{e:?}")))?;
    Ok(doc.page_count() as u32)
}

thread_local! {
    // pdfium-render promotes the first bindings into a process-wide OnceCell
    // and refuses a second bind, so one Pdfium lives for the worker's life.
    // A thread_local is enough: a wasm32-unknown-unknown module is one thread.
    static PDFIUM: OnceCell<Pdfium> = const { OnceCell::new() };
}

fn with_pdfium<T>(f: impl FnOnce(&Pdfium) -> Result<T, JsError>) -> Result<T, JsError> {
    PDFIUM.with(|cell| {
        if cell.get().is_none() {
            let bindings = Pdfium::bind_to_system_library()
                .map_err(|e| JsError::new(&format!("bind pdfium: {e:?}")))?;
            let _ = cell.set(Pdfium::new(bindings));
        }
        f(cell.get().expect("just set"))
    })
}

/// A rendered page: tightly packed RGBA, `width * height * 4` bytes.
#[wasm_bindgen]
pub struct RenderedPage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[wasm_bindgen]
impl RenderedPage {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }
    /// Moves the pixels out (one copy into a JS Uint8Array).
    pub fn take_rgba(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.rgba)
    }
}

/// Renders page `index` of `bytes` at `target_width` pixels wide.
#[wasm_bindgen]
pub fn render_page_rgba(bytes: &[u8], index: u16, target_width: i32) -> Result<RenderedPage, JsError> {
    console_error_panic_hook::set_once();
    with_pdfium(|pdfium| {
        let document = pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|e| JsError::new(&format!("load: {e:?}")))?;
        let page = document
            .pages()
            .get(index as _)
            .map_err(|e| JsError::new(&format!("page: {e:?}")))?;
        let config = PdfRenderConfig::new()
            .set_target_width(target_width)
            .render_form_data(true)
            .render_annotations(true);
        let bitmap = page
            .render_with_config(&config)
            .map_err(|e| JsError::new(&format!("render: {e:?}")))?;
        Ok(RenderedPage {
            width: bitmap.width() as u32,
            height: bitmap.height() as u32,
            rgba: bitmap.as_rgba_bytes(),
        })
    })
}
