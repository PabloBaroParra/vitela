// Text reads for the `Engine` contract: search and selection. The core
// answers in the page's unrotated PDF space; everything leaving here is on
// the page as drawn (top-left origin, `/Rotate` applied), through the core's
// own placement transform (core/pdf-wasm `placeRects` / `pointToPdf`), so
// the UI only scales by its zoom and no shell re-derives the rotation math.
//
// A drag-select asks for a selection on every pointer move, so each page's
// characters are built once and kept: a few pages' worth, freed on eviction
// and when their document closes.

import type { DocumentHandle, PagePoint, PageSize, SearchHit, TextSelection } from '../core/types';

/** A wasm-bindgen object the caller must free. */
export interface Owned {
  free(): void;
}

/** core/pdf-wasm's `PageCharacters`: carets index characters (0..=length). */
export interface CorePageCharacters extends Owned {
  caretAt(xPt: number, yPt: number): number | undefined;
  wordAt(xPt: number, yPt: number): Uint32Array | undefined;
  textIn(anchor: number, focus: number): string;
  rectsIn(anchor: number, focus: number): Float32Array;
}

/** The text slice of core/pdf-wasm's `Engine`. */
export interface CoreText {
  search(doc: number, query: string): (Owned & { readonly pageIndex: number; readonly lines: Float32Array })[];
  pageCharacters(doc: number, page: number): CorePageCharacters;
}

/** core/pdf-wasm's free placement functions. */
export interface CoreGeometry {
  placeRects(rects: Float32Array, widthPt: number, heightPt: number, rotationDegrees: number): Float32Array;
  pointToPdf(x: number, y: number, widthPt: number, heightPt: number, rotationDegrees: number): Float32Array;
}

/** How many pages' characters are kept: a drag stays on one page. */
const CACHED_PAGES = 4;

const EMPTY: TextSelection = { text: '', rects: new Float32Array(0) };

export class PageText {
  /** Most recently used last. Key: `doc/page`. */
  private readonly cache = new Map<string, { doc: number; characters: CorePageCharacters }>();

  constructor(
    private readonly core: CoreText,
    private readonly geometry: CoreGeometry,
  ) {}

  search(document: DocumentHandle, pages: readonly PageSize[], query: string): SearchHit[] {
    const matches = this.core.search(document, query);
    try {
      return matches.map(({ pageIndex, lines }) => {
        const page = pages[pageIndex];
        if (!page) throw new Error(`search reported page ${pageIndex}, past the last page`);
        return { pageIndex, rects: this.place(lines, page) };
      });
    } finally {
      for (const match of matches) match.free();
    }
  }

  selectText(document: DocumentHandle, pageIndex: number, page: PageSize, anchor: PagePoint, focus: PagePoint): TextSelection {
    const characters = this.characters(document, pageIndex);
    const from = this.caretAt(characters, page, anchor);
    const to = this.caretAt(characters, page, focus);
    if (from === undefined || to === undefined || from === to) return EMPTY;
    return this.between(characters, page, from, to);
  }

  selectWord(document: DocumentHandle, pageIndex: number, page: PageSize, point: PagePoint): TextSelection {
    const characters = this.characters(document, pageIndex);
    const [x, y] = this.geometry.pointToPdf(point.x, point.y, page.widthPt, page.heightPt, page.rotationDegrees);
    const word = characters.wordAt(x!, y!);
    if (!word || word[0] === word[1]) return EMPTY;
    return this.between(characters, page, word[0]!, word[1]!);
  }

  /** Frees every cached page of `document`. */
  forget(document: DocumentHandle): void {
    for (const [key, entry] of this.cache) {
      if (entry.doc === document) {
        entry.characters.free();
        this.cache.delete(key);
      }
    }
  }

  private characters(doc: number, pageIndex: number): CorePageCharacters {
    const key = `${doc}/${pageIndex}`;
    const cached = this.cache.get(key);
    if (cached) {
      // Re-insert: most recently used last.
      this.cache.delete(key);
      this.cache.set(key, cached);
      return cached.characters;
    }
    // Throws (TextExtractionDenied, PageIndexOutOfBounds...) before caching.
    const characters = this.core.pageCharacters(doc, pageIndex);
    this.cache.set(key, { doc, characters });
    if (this.cache.size > CACHED_PAGES) {
      const [oldest, entry] = this.cache.entries().next().value!;
      entry.characters.free();
      this.cache.delete(oldest);
    }
    return characters;
  }

  private caretAt(characters: CorePageCharacters, page: PageSize, point: PagePoint): number | undefined {
    const [x, y] = this.geometry.pointToPdf(point.x, point.y, page.widthPt, page.heightPt, page.rotationDegrees);
    return characters.caretAt(x!, y!);
  }

  private between(characters: CorePageCharacters, page: PageSize, from: number, to: number): TextSelection {
    return { text: characters.textIn(from, to), rects: this.place(characters.rectsIn(from, to), page) };
  }

  private place(rects: Float32Array, page: PageSize): Float32Array {
    return this.geometry.placeRects(rects, page.widthPt, page.heightPt, page.rotationDegrees);
  }
}
