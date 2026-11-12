// Domain types shared by the UI and the engine worker.

/**
 * One page's size in PDF points as the viewer lays it out — already turned
 * by its rotation, so a `/Rotate 90` portrait page reports landscape — plus
 * that clockwise turn (0, 90, 180 or 270). Straight from the core's
 * `pageDimensions`.
 */
export interface PageSize {
  readonly widthPt: number;
  readonly heightPt: number;
  readonly rotationDegrees: number;
}

/** What the engine reports about a freshly opened document. */
export interface DocumentInfo {
  readonly pageCount: number;
  /** One entry per page, in page order. */
  readonly pages: readonly PageSize[];
}

/** One rendered page: tightly packed RGBA, `width * height * 4` bytes. */
export interface RenderedPage {
  readonly width: number;
  readonly height: number;
  readonly rgba: Uint8ClampedArray;
}

/** What the UI may ask the engine to do. */
export interface Engine {
  /**
   * Opens `bytes` (with `password` for an encrypted document) and closes the
   * previously open one — but only once the new one opened, so a failed or
   * refused open leaves the previous document in place.
   */
  open(bytes: Uint8Array, password?: string): Promise<DocumentInfo>;
  /** Renders a whole page at `dpi` (72 = one pixel per PDF point). */
  renderPage(pageIndex: number, dpi: number): Promise<RenderedPage>;
}
