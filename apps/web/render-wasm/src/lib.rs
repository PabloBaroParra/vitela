//! TEMPORARY render entry point for the Vitela web worker (B26 T-220).
//!
//! It stands in for `core/pdf-wasm` (T-219) and is deliberately tiny: open a
//! document from bytes, report its page count, render one page to RGBA. The
//! worker (`apps/web/src/worker/engine.ts`) is its only caller, so replacing
//! it with `pdf-wasm` touches the worker alone.
//!
//! pdfium itself is a separate Emscripten module. JS must load it, apply the
//! `_PDFium_Init` alias and call `initialize_pdfium_render(pdfiumModule,
//! rustModule, debug)` (exported by pdfium-render) before anything here runs.

use std::cell::RefCell;
use std::sync::OnceLock;

use pdfium_render::prelude::*;
use wasm_bindgen::prelude::*;

/// pdfium-render refuses a second bind, so one `Pdfium` lives for the
/// worker's lifetime. Leaking it gives documents a `'static` borrow, which is
/// what lets the open document sit in a thread-local between calls.
fn pdfium() -> Result<&'static Pdfium, JsError> {
    static PDFIUM: OnceLock<&'static Pdfium> = OnceLock::new();
    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }
    let bindings = Pdfium::bind_to_system_library()
        .map_err(|e| JsError::new(&format!("bind pdfium: {e:?}")))?;
    Ok(PDFIUM.get_or_init(|| Box::leak(Box::new(Pdfium::new(bindings)))))
}

thread_local! {
    // A wasm32-unknown-unknown module runs on one thread: the worker's.
    static DOCUMENT: RefCell<Option<PdfDocument<'static>>> = const { RefCell::new(None) };
}

/// Opens `bytes` (replacing any open document) and returns its page count.
#[wasm_bindgen]
pub fn open_document(bytes: Vec<u8>) -> Result<u32, JsError> {
    console_error_panic_hook::set_once();
    let document = pdfium()?
        .load_pdf_from_byte_vec(bytes, None)
        .map_err(|e| JsError::new(&format!("open: {e:?}")))?;
    let pages = u32::try_from(document.pages().len()).unwrap_or(0);
    DOCUMENT.with(|slot| *slot.borrow_mut() = Some(document));
    Ok(pages)
}

/// Closes the open document, if any.
#[wasm_bindgen]
pub fn close_document() {
    DOCUMENT.with(|slot| *slot.borrow_mut() = None);
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

    /// Moves the pixels out (one copy into a JS `Uint8Array`).
    pub fn take_rgba(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.rgba)
    }
}

/// Renders page `index` of the open document at `target_width` pixels wide.
#[wasm_bindgen]
pub fn render_page(index: u32, target_width: i32) -> Result<RenderedPage, JsError> {
    DOCUMENT.with(|slot| {
        let slot = slot.borrow();
        let document = slot
            .as_ref()
            .ok_or_else(|| JsError::new("render: no document is open"))?;
        let page = document
            .pages()
            .get(
                PdfPageIndex::try_from(index)
                    .map_err(|_| JsError::new("page index out of range"))?,
            )
            .map_err(|e| JsError::new(&format!("page {index}: {e:?}")))?;
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
