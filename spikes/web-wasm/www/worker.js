// Loads two independent WASM modules into this worker:
//  - pdfium/pdfium.js: bblanchon's Emscripten build of PDFium chromium/7763
//    (non-MODULARIZE: it fills a global `Module`, so we pre-seed one).
//  - pkg/web_wasm_spike.js: our Rust crate (pdf-manip + pdfium-render),
//    wasm-bindgen --target no-modules (global `wasm_bindgen`).
const t0 = performance.now();
// ?cold appends a unique query to every asset (fixture included) so the
// HTTP cache cannot help.
const bust = new URLSearchParams(location.search).has('cold') ? '?t=' + Date.now() : '';
const stage = (name) => self.postMessage({ stage: name, atMs: performance.now() - t0 });
self.SPIKE_DEBUG = new URLSearchParams(location.search).has('debug');
const pdfiumReady = new Promise((resolve, reject) => {
  self.Module = {
    locateFile: (path) => 'pdfium/' + path + bust,
    onRuntimeInitialized: () => {
      // pdfium-render 0.9.5's WASM FPDF_InitLibrary() ccalls either
      // `PDF_InitLibrary` (if Module.FPDF_InitLibrary exists) or
      // `PDFium_Init` (paulocoutinhox/pdfium-lib's wrapper). bblanchon's
      // chromium/7763 build exports neither, only the real
      // `_FPDF_InitLibrary`, so alias it. (wasm_bindings.rs:1652-1668)
      self.Module._PDFium_Init = self.Module._FPDF_InitLibrary;
      resolve(self.Module);
    },
    // Without this a failed runtime (404 on pdfium.wasm, bad module, OOM)
    // never fires onRuntimeInitialized and the worker waits forever.
    onAbort: (what) => reject(new Error('pdfium runtime aborted: ' + what)),
  };
});
importScripts('pdfium/pdfium.js' + bust, 'pkg/web_wasm_spike.js' + bust);

self.onmessage = async ({ data }) => {
  try {
    const marks = {};
    const [pdfiumModule, rustModule, bytes] = await Promise.all([
      pdfiumReady.then((m) => { marks.pdfiumReadyMs = performance.now() - t0; return m; }),
      wasm_bindgen({ module_or_path: 'pkg/web_wasm_spike_bg.wasm' + bust }).then((m) => { marks.rustReadyMs = performance.now() - t0; return m; }),
      fetch(data.fixture + bust).then((r) => {
        if (!r.ok) throw new Error(`fixture ${data.fixture}: HTTP ${r.status}`);
        return r.arrayBuffer();
      }).then((b) => new Uint8Array(b)),
    ]);
    stage('modules loaded');
    if (!wasm_bindgen.initialize_pdfium_render(pdfiumModule, rustModule, self.SPIKE_DEBUG === true)) {
      throw new Error('initialize_pdfium_render returned false');
    }
    stage('pdfium-render bound');
    const tCore = performance.now();
    const corePages = wasm_bindgen.core_page_count(bytes);
    marks.coreOpenMs = performance.now() - tCore;

    stage('core opened');
    const tRender = performance.now();
    const page = wasm_bindgen.render_page_rgba(bytes, data.page, data.width);
    const rgba = page.take_rgba();
    marks.renderMs = performance.now() - tRender;
    const out = { page: data.page, bytes: bytes.length, width: page.width, height: page.height, corePages, ...marks,
                  workerTotalMs: performance.now() - t0, rgba: rgba.buffer };
    page.free();
    self.postMessage(out, [rgba.buffer]);
  } catch (e) {
    self.postMessage({ error: String(e && e.stack || e) });
  }
};
