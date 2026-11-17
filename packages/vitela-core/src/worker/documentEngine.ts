// The `Engine` contract over the core's document table. The core already
// keeps documents by id and refuses an id it does not hold, so the id is the
// handle: a render always reaches the document the UI asked for, never
// "whichever one is newest". Kept apart from engine.ts (which loads the WASM)
// so it is tested with a fake core.
//
// Page edits refresh the preview inside the same request: the core edits its
// model at once but renders and reads text from a pdfium snapshot that only
// `refreshPreview` brings up to date, and the worker answers one request at a
// time, so no render can ever land between an edit and its refresh.

import type {
  DocumentHandle,
  DocumentInfo,
  EditedDocument,
  Engine,
  OpenedDocument,
  PageEdit,
  PagePoint,
  PageSize,
  RenderedPage,
} from '../core/types.js';
import { PageText, type CoreGeometry, type CoreText, type Owned } from './pageText.js';

/** core/pdf-wasm's `EditCommand` constructors (the commands are wasm objects). */
export interface CoreEdits {
  movePages(from: number, count: number, to: number): Owned;
  rotatePages(from: number, count: number, deltaDegrees: number): Owned;
  removePages(index: number, count: number): Owned;
}

/** The slice of core/pdf-wasm's `Engine` this module uses. */
export interface CoreEngine extends CoreText {
  open(bytes: Uint8Array, password?: string): number;
  close(doc: number): boolean;
  pageCount(doc: number): number;
  pageDimensions(doc: number): (PageSize & Owned)[];
  pageIds(doc: number): Uint32Array;
  textExtractionAllowed(doc: number): boolean;
  documentAssemblyAllowed(doc: number): boolean;
  saveToBytes(doc: number, invalidateSignatures: boolean): Uint8Array;
  applyEdit(doc: number, edit: Owned): void;
  undo(doc: number): boolean;
  redo(doc: number): boolean;
  canUndo(doc: number): boolean;
  canRedo(doc: number): boolean;
  refreshPreview(doc: number): void;
  renderPage(doc: number, page: number, dpi: number, invert: boolean): Owned & {
    readonly width: number;
    readonly height: number;
    takeRgba(): Uint8Array;
  };
}

export function documentEngine(core: CoreEngine, geometry: CoreGeometry, edits: CoreEdits): Engine {
  const text = new PageText(core, geometry);
  /** Each open document's page sizes: the placement of its text needs them. */
  const sizes = new Map<DocumentHandle, readonly PageSize[]>();
  const pagesOf = (document: DocumentHandle) => {
    const pages = sizes.get(document);
    // The core's own refusal, for a document this side never opened.
    if (!pages) throw named('DocumentNotFound', `document ${document} is not open`);
    return pages;
  };
  /** Refuses a document this side never opened, with the core's own error. */
  const assertOpen = (document: DocumentHandle): void => void pagesOf(document);
  const pageOf = (document: DocumentHandle, pageIndex: number) => {
    const page = pagesOf(document)[pageIndex];
    if (!page) throw named('PageIndexOutOfBounds', `page ${pageIndex} is out of bounds`);
    return page;
  };

  /** What the document is now; the text placement follows its new page sizes. */
  const describe = (doc: DocumentHandle): DocumentInfo => {
    const info = documentInfo(core, doc);
    sizes.set(doc, info.pages);
    return info;
  };
  /**
   * Runs one step of the edit history, then shows it. If the preview cannot be
   * rebuilt, `revert` takes the step back, so the page list the UI is given
   * never disagrees with the pixels it renders.
   */
  const step = (document: DocumentHandle, run: () => boolean, revert: () => void): EditedDocument => {
    assertOpen(document);
    if (run()) {
      // Cached characters belong to pages that may have moved or gone.
      text.forget(document);
      try {
        core.refreshPreview(document);
      } catch (error) {
        revert();
        throw error;
      }
    }
    return { info: describe(document), canUndo: core.canUndo(document), canRedo: core.canRedo(document) };
  };

  return {
    async open(bytes: Uint8Array, password?: string): Promise<OpenedDocument> {
      const doc = core.open(bytes, password);
      try {
        return { handle: doc, info: describe(doc) };
      } catch (error) {
        core.close(doc);
        throw error;
      }
    },
    async renderPage(document: DocumentHandle, pageIndex: number, dpi: number): Promise<RenderedPage> {
      const page = core.renderPage(document, pageIndex, dpi, false);
      try {
        const rgba = page.takeRgba();
        return {
          width: page.width,
          height: page.height,
          rgba: new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.byteLength),
        };
      } finally {
        page.free();
      }
    },
    async close(document: DocumentHandle): Promise<void> {
      text.forget(document);
      sizes.delete(document);
      core.close(document);
    },
    async saveToBytes(document: DocumentHandle, invalidateSignatures: boolean) {
      assertOpen(document);
      return core.saveToBytes(document, invalidateSignatures);
    },
    async applyEdit(document: DocumentHandle, edit: PageEdit) {
      return step(
        document,
        () => {
          const command = editCommand(edits, edit);
          try {
            core.applyEdit(document, command);
          } finally {
            command.free();
          }
          return true;
        },
        () => void core.undo(document),
      );
    },
    async undo(document: DocumentHandle) {
      return step(document, () => core.undo(document), () => void core.redo(document));
    },
    async redo(document: DocumentHandle) {
      return step(document, () => core.redo(document), () => void core.undo(document));
    },
    async search(document: DocumentHandle, query: string) {
      return text.search(document, pagesOf(document), query);
    },
    async selectText(document: DocumentHandle, pageIndex: number, anchor: PagePoint, focus: PagePoint) {
      return text.selectText(document, pageIndex, pageOf(document, pageIndex), anchor, focus);
    },
    async selectWord(document: DocumentHandle, pageIndex: number, point: PagePoint) {
      return text.selectWord(document, pageIndex, pageOf(document, pageIndex), point);
    },
  };
}

/** An `Error` named like the core's own refusals, so the UI branches alike. */
function named(name: string, message: string): Error {
  const error = new Error(message);
  error.name = name;
  return error;
}

function editCommand(edits: CoreEdits, edit: PageEdit): Owned {
  switch (edit.kind) {
    case 'move':
      return edits.movePages(edit.from, edit.count, edit.to);
    case 'rotate':
      return edits.rotatePages(edit.from, edit.count, edit.degrees);
    case 'remove':
      return edits.removePages(edit.from, edit.count);
  }
}

function documentInfo(core: CoreEngine, doc: number): DocumentInfo {
  return {
    pageCount: core.pageCount(doc),
    pages: pageSizes(core, doc),
    pageIds: Array.from(core.pageIds(doc)),
    textExtractionAllowed: core.textExtractionAllowed(doc),
    documentAssemblyAllowed: core.documentAssemblyAllowed(doc),
  };
}

/** Copies the core's page sizes into plain objects and frees the wasm ones. */
function pageSizes(core: CoreEngine, doc: number): PageSize[] {
  return core.pageDimensions(doc).map((dimensions) => {
    try {
      const { widthPt, heightPt, rotationDegrees } = dimensions;
      return { widthPt, heightPt, rotationDegrees };
    } finally {
      dimensions.free();
    }
  });
}
