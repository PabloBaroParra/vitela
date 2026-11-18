import { describe, expect, it } from 'vitest';
import { PageFrame, annotations, comments, packRgb, unpackRgb, type CoreAnnotation } from './annotations.js';
import type { CoreGeometry } from './pageText.js';

/** An upright page: PDF y grows up from the bottom, drawn y down from the top. */
const flipping: CoreGeometry = {
  placeRects: (rects, _width, height) => {
    const placed = new Float32Array(rects.length);
    for (let i = 0; i + 3 < rects.length; i += 4) {
      placed.set([rects[i]!, height - rects[i + 1]! - rects[i + 3]!, rects[i + 2]!, rects[i + 3]!], i);
    }
    return placed;
  },
  pointToPdf: (x, y, _width, height) => new Float32Array([x, height - y]),
};
const page = new PageFrame({ widthPt: 612, heightPt: 792, rotationDegrees: 0 }, flipping);

function entry(fields: Partial<CoreAnnotation>, freed: string[] = []): CoreAnnotation {
  return {
    id: 0,
    page: 0,
    kind: 'highlight',
    rect: new Float64Array([10, 700, 100, 20]),
    points: new Float64Array(0),
    color: 0xff0000,
    contents: '',
    layout: undefined,
    free: () => freed.push(`entry:${fields.id ?? 0}`),
    ...fields,
  };
}

describe('packRgb / unpackRgb', () => {
  it('packs each channel into 0xRRGGBB, rounded and clamped', () => {
    expect(packRgb({ r: 0x12, g: 0x80, b: 0xfe })).toBe(0x1280fe);
    expect(packRgb({ r: 256, g: -1, b: 9.6 })).toBe(0xff000a);
    expect(unpackRgb(0x1280fe)).toEqual({ r: 0x12, g: 0x80, b: 0xfe });
  });
});

describe('PageFrame', () => {
  it('takes a drawn box to PDF space between its two corners, and back', () => {
    const box = { left: 10, top: 72, width: 100, height: 20 };
    expect(page.rectToPdf(box)).toEqual([10, 700, 100, 20]);
    expect(page.rectFromPdf([10, 700, 100, 20])).toEqual(box);
  });

  it('moves a displacement by the linear part only', () => {
    expect(page.deltaToPdf(5, 10)).toEqual([5, -10]);
  });

  it('places points as empty boxes and ignores a lone value', () => {
    expect(page.pointsFromPdf([10, 772, 30, 742, 99])).toEqual([
      { x: 10, y: 20 },
      { x: 30, y: 50 },
    ]);
  });
});

describe('annotations', () => {
  it('decodes each kind and frees every entry, skipping one it does not know', () => {
    const freed: string[] = [];
    const decoded = annotations(
      [
        entry({ id: 1 }, freed),
        entry({ id: 2, kind: 'polygon' }, freed),
        entry({ id: 3, kind: 'stamp', color: undefined }, freed),
      ],
      () => page,
    );
    expect(decoded).toEqual([
      { kind: 'highlight', id: 1, pageIndex: 0, rect: { left: 10, top: 72, width: 100, height: 20 }, color: { r: 255, g: 0, b: 0 } },
      { kind: 'stamp', id: 3, pageIndex: 0, rect: { left: 10, top: 72, width: 100, height: 20 } },
    ]);
    expect(freed).toEqual(['entry:1', 'entry:2', 'entry:3']);
  });

  it('skips an entry missing what its kind needs, or on a page it has no frame for', () => {
    expect(
      annotations(
        [
          entry({ id: 1, color: undefined }),
          entry({ id: 2, kind: 'freeText', layout: undefined }),
          entry({ id: 3, rect: new Float64Array(0) }),
          entry({ id: 4, kind: 'ink', color: undefined }),
          entry({ id: 5, page: 7 }),
        ],
        (index) => (index === 0 ? page : undefined),
      ),
    ).toEqual([]);
  });
});

describe('comments', () => {
  it('turns wasm-bindgen absent values into null', () => {
    const freed: string[] = [];
    expect(
      comments(
        [
          {
            page: 0,
            rect: new Float64Array([40, 600, 20, 20]),
            contents: 'Hi',
            author: undefined,
            date: undefined,
            annotationId: undefined,
            free: () => freed.push('comment'),
          },
        ],
        () => page,
      ),
    ).toEqual([
      { pageIndex: 0, rect: { left: 40, top: 172, width: 20, height: 20 }, contents: 'Hi', author: null, date: null, annotationId: null },
    ]);
    expect(freed).toEqual(['comment']);
  });
});
