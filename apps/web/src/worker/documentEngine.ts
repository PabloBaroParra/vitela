// The `Engine` contract over the core's document table. The core already
// keeps documents by id and refuses an id it does not hold, so the id is the
// handle: a render always reaches the document the UI asked for, never
// "whichever one is newest". Kept apart from engine.ts (which loads the WASM)
// so it is tested with a fake core.

import type { DocumentHandle, Engine, OpenedDocument, PagePoint, PageSize, RenderedPage } from '../core/types';
import { PageText, type CoreGeometry, type CoreText, type Owned } from './pageText';

/** The slice of core/pdf-wasm's `Engine` this module uses. */
export interface CoreEngine extends CoreText {
  open(bytes: Uint8Array, password?: string): number;
  close(doc: number): boolean;
  pageCount(doc: number): number;
  pageDimensions(doc: number): (PageSize & Owned)[];
  textExtractionAllowed(doc: number): boolean;
  renderPage(doc: number, page: number, dpi: number, invert: boolean): Owned & {
    readonly width: number;
    readonly height: number;
    takeRgba(): Uint8Array;
  };
}

export function documentEngine(core: CoreEngine, geometry: CoreGeometry): Engine {
  const text = new PageText(core, geometry);
  /** Each open document's page sizes: the placement of its text needs them. */
  const sizes = new Map<DocumentHandle, readonly PageSize[]>();
  const pagesOf = (document: DocumentHandle) => {
    const pages = sizes.get(document);
    // The core's own refusal, for a document this side never opened.
    if (!pages) throw named('DocumentNotFound', `document ${document} is not open`);
    return pages;
  };
  const pageOf = (document: DocumentHandle, pageIndex: number) => {
    const page = pagesOf(document)[pageIndex];
    if (!page) throw named('PageIndexOutOfBounds', `page ${pageIndex} is out of bounds`);
    return page;
  };

  return {
    async open(bytes: Uint8Array, password?: string): Promise<OpenedDocument> {
      const doc = core.open(bytes, password);
      try {
        const pages = pageSizes(core, doc);
        const info = { pageCount: core.pageCount(doc), pages, textExtractionAllowed: core.textExtractionAllowed(doc) };
        sizes.set(doc, pages);
        return { handle: doc, info };
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
