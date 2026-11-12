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
  /**
   * Whether the document lets its text be read (PDF permission bit 5, or
   * opened with the owner password). When `false` the engine refuses every
   * text read with `TextExtractionDenied`, and the UI offers neither search
   * nor selection.
   */
  readonly textExtractionAllowed: boolean;
}

/** A point on a page as it is drawn, in PDF points from its top-left corner. */
export interface PagePoint {
  readonly x: number;
  readonly y: number;
}

/**
 * Rects on a page as it is drawn, flattened: `left, top, width, height` per
 * rect, in PDF points from the page's top-left corner, with the page's
 * `/Rotate` already applied. The viewer only scales them by its zoom.
 */
export type PageRects = Float32Array;

/** One exact match of a search: its page and one rect per line it spans. */
export interface SearchHit {
  readonly pageIndex: number;
  readonly rects: PageRects;
}

/** Selected text on one page and the rects to paint it, one per line. */
export interface TextSelection {
  readonly text: string;
  readonly rects: PageRects;
}

/**
 * Names one open document in the engine. Every render says which document
 * it is for, so the engine can never paint a document the UI is not showing.
 */
export type DocumentHandle = number;

/** A freshly opened document: its handle and what the engine reports about it. */
export interface OpenedDocument {
  readonly handle: DocumentHandle;
  readonly info: DocumentInfo;
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
   * Opens `bytes` (with `password` for an encrypted document). Documents
   * already open stay open: the caller closes each one it no longer shows.
   */
  open(bytes: Uint8Array, password?: string): Promise<OpenedDocument>;
  /**
   * Renders a whole page of `document` at `dpi` (72 = one pixel per PDF
   * point). Rejects (`DocumentNotFound`) once that document was closed.
   */
  renderPage(document: DocumentHandle, pageIndex: number, dpi: number): Promise<RenderedPage>;
  /** Closes `document`; closing one that is not open does nothing. */
  close(document: DocumentHandle): Promise<void>;
  /**
   * Every exact, case-sensitive match of `query` (the core's search), in page
   * order. An empty query matches nothing. Rejects with `TextExtractionDenied`
   * when the document withholds its text.
   */
  search(document: DocumentHandle, query: string): Promise<SearchHit[]>;
  /**
   * The text between the characters nearest `anchor` and `focus` on one
   * page, in either order; empty when they meet or the page has no text.
   * Same refusal as `search`.
   */
  selectText(document: DocumentHandle, pageIndex: number, anchor: PagePoint, focus: PagePoint): Promise<TextSelection>;
  /** The word under `point`; empty on whitespace. Same refusal as `search`. */
  selectWord(document: DocumentHandle, pageIndex: number, point: PagePoint): Promise<TextSelection>;
}
