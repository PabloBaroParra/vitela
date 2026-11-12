import { describe, expect, it } from 'vitest';
import { PageText, type CoreGeometry, type CorePageCharacters, type CoreText } from './pageText';

const LETTER = { widthPt: 612, heightPt: 792, rotationDegrees: 90 };

function fakeCore() {
  const freed: string[] = [];
  const text: CoreText = {
    search: () => [],
    pageCharacters: (doc, page): CorePageCharacters => ({
      caretAt: (x) => x,
      wordAt: () => undefined,
      textIn: () => 'x',
      rectsIn: () => new Float32Array([0, 0, 1, 1]),
      free: () => freed.push(`${doc}/${page}`),
    }),
  };
  return { text, freed };
}

/** Records what it was asked to place, and places nothing. */
function recordingGeometry() {
  const calls: unknown[][] = [];
  const geometry: CoreGeometry = {
    placeRects: (...args) => (calls.push(['placeRects', ...args.slice(1)]), args[0]),
    pointToPdf: (...args) => (calls.push(['pointToPdf', ...args]), new Float32Array([args[0], args[1]])),
  };
  return { geometry, calls };
}

describe('PageText', () => {
  it("hands the core's placement the page as drawn, turn included", () => {
    const { text } = fakeCore();
    const { geometry, calls } = recordingGeometry();
    new PageText(text, geometry).selectText(1, 0, LETTER, { x: 0, y: 0 }, { x: 2, y: 0 });
    expect(calls).toEqual([
      ['pointToPdf', 0, 0, 612, 792, 90],
      ['pointToPdf', 2, 0, 612, 792, 90],
      ['placeRects', 612, 792, 90],
    ]);
  });

  it('keeps the four most recently used pages and frees the oldest', () => {
    const { text, freed } = fakeCore();
    const pages = new PageText(text, recordingGeometry().geometry);
    const select = (page: number) => pages.selectText(1, page, LETTER, { x: 0, y: 0 }, { x: 1, y: 0 });
    [0, 1, 2, 3].forEach(select);
    select(0); // page 0 is now the most recent: page 1 goes first
    select(4);
    expect(freed).toEqual(['1/1']);
  });

  it('forgets only the closed document', () => {
    const { text, freed } = fakeCore();
    const pages = new PageText(text, recordingGeometry().geometry);
    pages.selectText(1, 0, LETTER, { x: 0, y: 0 }, { x: 1, y: 0 });
    pages.selectText(2, 0, LETTER, { x: 0, y: 0 }, { x: 1, y: 0 });
    pages.forget(1);
    expect(freed).toEqual(['1/0']);
  });

  it('refuses a match on a page the document does not have, and still frees it', () => {
    let freed = 0;
    const text: CoreText = {
      search: () => [{ pageIndex: 3, lines: new Float32Array(0), free: () => freed++ }],
      pageCharacters: () => {
        throw new Error('unused');
      },
    };
    expect(() => new PageText(text, recordingGeometry().geometry).search(1, [LETTER], 'x')).toThrow(
      'past the last page',
    );
    expect(freed).toBe(1);
  });
});
