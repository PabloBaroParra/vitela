// Domain types shared by the UI and the engine worker.

/** What the engine reports about a freshly opened document. */
export interface DocumentInfo {
  readonly pageCount: number;
}

/** One rendered page: tightly packed RGBA, `width * height * 4` bytes. */
export interface RenderedPage {
  readonly width: number;
  readonly height: number;
  readonly rgba: Uint8ClampedArray;
}

/** What the UI may ask the engine to do. */
export interface Engine {
  open(bytes: Uint8Array): Promise<DocumentInfo>;
  renderPage(pageIndex: number, targetWidth: number): Promise<RenderedPage>;
}
