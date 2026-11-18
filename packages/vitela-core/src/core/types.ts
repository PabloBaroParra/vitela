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
   * Each page's identity (the core's `PageId`), in page order. It follows
   * the page through moves and removals, so per-page state such as a
   * thumbnail is kept by id, never by position.
   */
  readonly pageIds: readonly number[];
  /**
   * Whether the document lets its text be read (PDF permission bit 5, or
   * opened with the owner password). When `false` the engine refuses every
   * text read with `TextExtractionDenied`, and the UI offers neither search
   * nor selection.
   */
  readonly textExtractionAllowed: boolean;
  /**
   * Whether the document lets its pages be moved, rotated or removed (the
   * PDF document-assembly permission, or the owner password). When `false`
   * the engine refuses every page edit with `EditNotPermitted`.
   */
  readonly documentAssemblyAllowed: boolean;
  /**
   * The document's pages grouped into blocks — one per contiguous run of
   * pages from the same PDF (the core's `derive_blocks`), in page order. What
   * Organize's Documents view shows.
   */
  readonly blocks: readonly DocumentBlock[];
  /**
   * Why the document would refuse any import right now, in the reader's
   * words, or `null` when it would not (the core's `import_refusal`: the
   * modify-contents and assembly permissions, and whether its encryption
   * survives the rewrite an import forces). Asked before the file picker.
   */
  readonly importRefusal: string | null;
}

/** Which PDF a block's pages come from. */
export type BlockSource =
  /** The PDF the document was opened from. */
  | { readonly kind: 'base' }
  /** Inserted blank pages. */
  | { readonly kind: 'blank' }
  /** A PDF added by `importPrepared`; `id` is one of its `sourceIds`. */
  | { readonly kind: 'imported'; readonly id: number };

/** One contiguous run of pages from one PDF. Positions are current until the next edit. */
export interface DocumentBlock {
  readonly source: BlockSource;
  /** 1-based, only when the PDF is split across more than one block; else `null`. */
  readonly part: number | null;
  /** Position of the block's first page. */
  readonly start: number;
  /** How many pages it holds (never zero). */
  readonly count: number;
}

/** One PDF opened as an import source, waiting for `importPrepared`. */
export interface PreparedImport {
  /** What `importPrepared` and `discardImport` name it by. */
  readonly id: number;
  /** How many pages importing it adds: every page it has. */
  readonly pageCount: number;
  /** What its pages will not bring across exactly as they are, one line each. */
  readonly warnings: readonly string[];
}

/** The document after an import, plus the block source id of each imported PDF, in pick order. */
export interface ImportedDocument extends EditedDocument {
  readonly sourceIds: readonly number[];
}

/**
 * One page edit, recorded as one undo step. Positions are 0-based and name
 * pages as they are now; `count` pages starting at `from` take part.
 */
export type PageEdit =
  /** Moves the run so its first page ends up at `to`. */
  | { readonly kind: 'move'; readonly from: number; readonly count: number; readonly to: number }
  /** Turns each page of the run clockwise by `degrees` (a multiple of 90). */
  | { readonly kind: 'rotate'; readonly from: number; readonly count: number; readonly degrees: number }
  | { readonly kind: 'remove'; readonly from: number; readonly count: number };

/** The document after an edit, undo or redo, and where its history stands. */
export interface EditedDocument {
  readonly info: DocumentInfo;
  readonly canUndo: boolean;
  readonly canRedo: boolean;
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
   * The document with every edit applied, as a complete PDF that keeps the
   * document's own encryption. A document whose signatures the save would
   * break rejects with `SignaturesWouldBeInvalidated` unless
   * `invalidateSignatures` says the user was told. Rejects with `SaveFailed`
   * when the bytes cannot be written.
   */
  saveToBytes(document: DocumentHandle, invalidateSignatures: boolean): Promise<Uint8Array>;
  /**
   * Records one page edit as one undo step and answers with the document as
   * it is now. Renders and text reads after it already show the edit. Rejects
   * with `EditNotPermitted` (no assembly permission), `PageIndexOutOfBounds`
   * or `UnsupportedOperation`, recording nothing.
   */
  applyEdit(document: DocumentHandle, edit: PageEdit): Promise<EditedDocument>;
  /** Takes back the last edit (nothing, when there is none). Same answer as `applyEdit`. */
  undo(document: DocumentHandle): Promise<EditedDocument>;
  /** Re-applies the last undone edit (nothing, when there is none). */
  redo(document: DocumentHandle): Promise<EditedDocument>;
  /**
   * Opens `bytes` as an import source (with `password` for an encrypted one)
   * and keeps it until `importPrepared` takes it or `discardImport` drops it.
   * Changes no document. Rejects with `PasswordRequired` or `WrongPassword`
   * when the source needs a password `password` is not (ask, then call
   * again), and `UnsupportedOperation` when it forbids copying its pages, has
   * none, or carries something an import cannot graft.
   */
  prepareImport(bytes: Uint8Array, password?: string): Promise<PreparedImport>;
  /** Drops a prepared source (an abandoned pick); dropping an unknown one does nothing. */
  discardImport(id: number): Promise<void>;
  /**
   * Adds every page of the prepared sources `ids`, in that order, at `index`
   * (the page count appends), as ONE undo step, and answers like `applyEdit`.
   * All or nothing: rejects with `EditNotPermitted`, `UnsupportedOperation`
   * or `PageIndexOutOfBounds` leaving every source prepared.
   */
  importPrepared(document: DocumentHandle, ids: readonly number[], index: number): Promise<ImportedDocument>;
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
