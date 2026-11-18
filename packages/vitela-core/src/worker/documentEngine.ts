// The `Engine` contract over the core's document table. The core already
// keeps documents by id and refuses an id it does not hold, so the id is the
// handle: a render always reaches the document the UI asked for, never
// "whichever one is newest". Kept apart from engine.ts (which loads the WASM)
// so it is tested with a fake core.
//
// Page edits refresh the preview inside the same request: the core edits its
// model at once but renders and reads text from a pdfium snapshot that only
// `refreshPreview` brings up to date, and the worker answers one request at a
// time, so no render can ever land between an edit and its refresh. An
// import is an edit like any other here: one step, refreshed the same way.
//
// Only when the core says the step changed the pixels (`previewStale`, the
// core's `shows_in_session_preview`): an annotation edit does not, because
// the preview leaves the session's annotations out and the UI draws them from
// the snapshot every answer carries. So adding a highlight costs no reopen,
// and undoing across a mixed history refreshes exactly on the page steps.

import type {
  Annotation,
  BlockSource,
  DocumentBlock,
  DocumentHandle,
  DocumentInfo,
  Edit,
  EditedDocument,
  Engine,
  ImportedDocument,
  OpenedDocument,
  PageEdit,
  PagePoint,
  PageSize,
  PreparedImport,
  RenderedPage,
} from '../core/types.js';
import {
  PageFrame,
  annotationCommand,
  annotations,
  comments,
  freeTextLayout,
  type CoreAnnotation,
  type CoreAnnotationEdits,
  type CoreComment,
  type CoreFreeTextLines,
} from './annotations.js';
import { PageText, type CoreGeometry, type CoreText, type Owned } from './pageText.js';

/** core/pdf-wasm's `EditCommand` constructors (the commands are wasm objects). */
export interface CoreEdits extends CoreAnnotationEdits {
  movePages(from: number, count: number, to: number): Owned;
  rotatePages(from: number, count: number, deltaDegrees: number): Owned;
  removePages(index: number, count: number): Owned;
}

/** core/pdf-wasm's free functions: placement, and the geometry asked before an annotation exists. */
export interface CoreFunctions extends CoreGeometry {
  freeTextLayout(contents: string, widthPt: number, heightPt: number): CoreFreeTextLines;
  /** `x, y, width, height` in PDF space. */
  stampPlacement(image: Uint8Array, anchorX: number, anchorY: number): Float64Array;
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
  annotationEditingAllowed(doc: number): boolean;
  saveToBytes(doc: number, invalidateSignatures: boolean): Uint8Array;
  /** The id of the annotation the edit added, else `undefined`. */
  applyEdit(doc: number, edit: Owned): number | undefined;
  undo(doc: number): boolean;
  redo(doc: number): boolean;
  canUndo(doc: number): boolean;
  canRedo(doc: number): boolean;
  refreshPreview(doc: number): void;
  /** Whether a step since the last refresh changed what the pixels must show. */
  previewStale(doc: number): boolean;
  annotations(doc: number): CoreAnnotation[];
  comments(doc: number): CoreComment[];
  prepareImport(bytes: Uint8Array, password?: string): Owned & {
    readonly id: number;
    readonly pageCount: number;
    readonly warnings: string[];
  };
  discardPrepared(id: number): boolean;
  importPrepared(doc: number, ids: Uint32Array, index: number): Owned & {
    readonly pageCount: number;
    readonly sourceIds: Uint32Array;
  };
  /** `undefined` (wasm-bindgen's `None`) when the document refuses no import. */
  importRefusal(doc: number): string | null | undefined;
  documentBlocks(doc: number): (Owned & {
    readonly source: string;
    readonly sourceId: number | undefined;
    readonly part: number | undefined;
    readonly start: number;
    readonly count: number;
  })[];
  renderPage(doc: number, page: number, dpi: number, invert: boolean): Owned & {
    readonly width: number;
    readonly height: number;
    takeRgba(): Uint8Array;
  };
}

export function documentEngine(core: CoreEngine, functions: CoreFunctions, edits: CoreEdits): Engine {
  const text = new PageText(core, functions);
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
  /** The frame of a page as the UI last saw it described, or `undefined` past the end. */
  const frameOf = (document: DocumentHandle) => (page: number) => {
    const size = sizes.get(document)?.[page];
    return size && new PageFrame(size, functions);
  };
  /** The session's annotations on the pages as drawn; call after `describe`. */
  const snapshot = (document: DocumentHandle): Annotation[] => annotations(core.annotations(document), frameOf(document));
  /**
   * Runs one step of the edit history, then shows it: refreshes the preview
   * when the core says the step changed the pixels. If the preview cannot be
   * rebuilt, `revert` takes the step back, so the page list the UI is given
   * never disagrees with the pixels it renders.
   */
  const step = (document: DocumentHandle, run: () => number | undefined, revert: () => void): EditedDocument => {
    assertOpen(document);
    const added = run();
    if (core.previewStale(document)) {
      // Cached characters belong to pages that may have moved or gone.
      text.forget(document);
      try {
        core.refreshPreview(document);
      } catch (error) {
        revert();
        throw error;
      }
    }
    const info = describe(document);
    return {
      info,
      canUndo: core.canUndo(document),
      canRedo: core.canRedo(document),
      annotations: snapshot(document),
      addedAnnotationId: added ?? null,
    };
  };
  /** Steps back or forth through the history (`run` answers whether it moved). */
  const history = (document: DocumentHandle, run: () => boolean, revert: () => void) =>
    step(
      document,
      () => {
        run();
        return undefined;
      },
      revert,
    );

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
    async applyEdit(document: DocumentHandle, edit: Edit) {
      return step(
        document,
        () => {
          assertOpen(document);
          const command = isPageEdit(edit)
            ? pageCommand(edits, edit)
            : annotationCommand(edits, edit, frameOf(document), (id) => annotationPage(document, id));
          try {
            return core.applyEdit(document, command);
          } finally {
            command.free();
          }
        },
        () => void core.undo(document),
      );
    },
    async undo(document: DocumentHandle) {
      return history(document, () => core.undo(document), () => void core.redo(document));
    },
    async redo(document: DocumentHandle) {
      return history(document, () => core.redo(document), () => void core.undo(document));
    },
    async prepareImport(bytes: Uint8Array, password?: string): Promise<PreparedImport> {
      const prepared = core.prepareImport(bytes, password);
      try {
        return { id: prepared.id, pageCount: prepared.pageCount, warnings: [...prepared.warnings] };
      } finally {
        prepared.free();
      }
    },
    async discardImport(id: number) {
      core.discardPrepared(id);
    },
    async importPrepared(document: DocumentHandle, ids: readonly number[], index: number): Promise<ImportedDocument> {
      let sourceIds: number[] = [];
      const edited = step(
        document,
        () => {
          const report = core.importPrepared(document, Uint32Array.from(ids), index);
          try {
            sourceIds = Array.from(report.sourceIds);
          } finally {
            report.free();
          }
          return undefined;
        },
        () => void core.undo(document),
      );
      return { ...edited, sourceIds };
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
    async annotations(document: DocumentHandle) {
      assertOpen(document);
      return snapshot(document);
    },
    async comments(document: DocumentHandle) {
      assertOpen(document);
      return comments(core.comments(document), frameOf(document));
    },
    async freeTextLayout(contents: string, widthPt: number, heightPt: number) {
      const lines = functions.freeTextLayout(contents, widthPt, heightPt);
      try {
        return freeTextLayout(lines);
      } finally {
        lines.free();
      }
    },
    async stampPlacement(document: DocumentHandle, pageIndex: number, image: Uint8Array, anchor: PagePoint) {
      const page = pageOf(document, pageIndex);
      // Only the size is the core's: it is asked at the PDF origin, and the
      // box is put with its drawn top-left corner at the anchor. A page drawn
      // a quarter-turn round shows the image turned, so its sides swap.
      const [, , width, height] = functions.stampPlacement(image, 0, 0);
      const turned = page.rotationDegrees % 180 !== 0;
      return { left: anchor.x, top: anchor.y, width: turned ? height! : width!, height: turned ? width! : height! };
    },
  };

  /** Where annotation `id`'s page is now, or `undefined` when the session holds no such annotation. */
  function annotationPage(document: DocumentHandle, id: number): number | undefined {
    let page: number | undefined;
    for (const entry of core.annotations(document)) {
      if (entry.id === id) page = entry.page;
      entry.free();
    }
    return page;
  }
}

function isPageEdit(edit: Edit): edit is PageEdit {
  return edit.kind === 'move' || edit.kind === 'rotate' || edit.kind === 'remove';
}

/** An `Error` named like the core's own refusals, so the UI branches alike. */
function named(name: string, message: string): Error {
  const error = new Error(message);
  error.name = name;
  return error;
}

function pageCommand(edits: CoreEdits, edit: PageEdit): Owned {
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
    blocks: documentBlocks(core, doc),
    importRefusal: core.importRefusal(doc) ?? null,
    annotationEditingAllowed: core.annotationEditingAllowed(doc),
  };
}

/** Copies the core's blocks into plain objects and frees the wasm ones. */
function documentBlocks(core: CoreEngine, doc: number): DocumentBlock[] {
  return core.documentBlocks(doc).map((block) => {
    try {
      return { source: blockSource(block.source, block.sourceId), part: block.part ?? null, start: block.start, count: block.count };
    } finally {
      block.free();
    }
  });
}

function blockSource(kind: string, id: number | undefined): BlockSource {
  if (kind === 'imported' && id !== undefined) return { kind: 'imported', id };
  return kind === 'blank' ? { kind: 'blank' } : { kind: 'base' };
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
