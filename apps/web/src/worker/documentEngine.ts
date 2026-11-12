// The `Engine` contract over the core's document table. The core already
// keeps documents by id and refuses an id it does not hold, so the id is the
// handle: a render always reaches the document the UI asked for, never
// "whichever one is newest". Kept apart from engine.ts (which loads the WASM)
// so it is tested with a fake core.

import type { DocumentHandle, Engine, OpenedDocument, PageSize, RenderedPage } from '../core/types';

/** A wasm-bindgen object the caller must free. */
interface Owned {
  free(): void;
}

/** The slice of core/pdf-wasm's `Engine` this module uses. */
export interface CoreEngine {
  open(bytes: Uint8Array, password?: string): number;
  close(doc: number): boolean;
  pageCount(doc: number): number;
  pageDimensions(doc: number): (PageSize & Owned)[];
  renderPage(doc: number, page: number, dpi: number, invert: boolean): Owned & {
    readonly width: number;
    readonly height: number;
    takeRgba(): Uint8Array;
  };
}

export function documentEngine(core: CoreEngine): Engine {
  return {
    async open(bytes: Uint8Array, password?: string): Promise<OpenedDocument> {
      const doc = core.open(bytes, password);
      try {
        return { handle: doc, info: { pageCount: core.pageCount(doc), pages: pageSizes(core, doc) } };
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
      core.close(document);
    },
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
