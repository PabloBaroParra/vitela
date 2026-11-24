import { describe, expect, it } from 'vitest';
import type { Annotation, PageSize } from '@vitela/core';
import {
  annotationBounds,
  canRecolor,
  canResize,
  canRetext,
  clampMove,
  cornerPoint,
  resizedBy,
  previewEdit,
  sameGeometry,
  annotationLabel,
  hitArea,
  HIT_SLOP_PT,
  keepsProportions,
  MIN_RESIZE_PT,
  resizedRect,
  resizeFloor,
} from './editGeometry';

const LETTER: PageSize = { widthPt: 612, heightPt: 792, rotationDegrees: 0 };
const RED = { r: 220, g: 40, b: 40 };
const box = { left: 100, top: 200, width: 80, height: 40 };

const highlight: Annotation = { id: 1, pageIndex: 0, kind: 'highlight', rect: box, color: RED };
const underline: Annotation = { id: 2, pageIndex: 0, kind: 'underline', rect: box, color: RED };
const shape: Annotation = { id: 3, pageIndex: 0, kind: 'shape', rect: box, color: RED };
const ink: Annotation = {
  id: 4,
  pageIndex: 0,
  kind: 'ink',
  points: [
    { x: 10, y: 50 },
    { x: 40, y: 20 },
    { x: 30, y: 70 },
  ],
  color: RED,
};
const note: Annotation = { id: 5, pageIndex: 0, kind: 'note', rect: box, contents: 'n' };
const stamp: Annotation = { id: 6, pageIndex: 0, kind: 'stamp', rect: box };
const freeText: Annotation = {
  id: 7,
  pageIndex: 0,
  kind: 'freeText',
  rect: box,
  contents: 't',
  layout: { fontSizePt: 12, lines: [], overflow: false },
};

describe('annotationBounds', () => {
  it('is the box of a boxed annotation and the extent of an ink stroke', () => {
    expect(annotationBounds(shape)).toEqual(box);
    expect(annotationBounds(ink)).toEqual({ left: 10, top: 20, width: 30, height: 50 });
  });
});

describe('hitArea', () => {
  it('grows thin marks (rules and strokes) so they can be grabbed, and leaves boxes alone', () => {
    expect(hitArea(highlight)).toEqual(box);
    expect(hitArea(underline)).toEqual({
      left: 100 - HIT_SLOP_PT,
      top: 200 - HIT_SLOP_PT,
      width: 80 + 2 * HIT_SLOP_PT,
      height: 40 + 2 * HIT_SLOP_PT,
    });
    expect(hitArea(ink).left).toBe(10 - HIT_SLOP_PT);
  });
});

describe('what each kind can do (the core refuses the rest)', () => {
  it('resizes everything but ink', () => {
    expect([highlight, underline, shape, note, stamp, freeText].every(canResize)).toBe(true);
    expect(canResize(ink)).toBe(false);
  });

  it('recolours markup, shapes and ink only', () => {
    expect([highlight, underline, shape, ink].every(canRecolor)).toBe(true);
    expect([note, stamp, freeText].some(canRecolor)).toBe(false);
  });

  it('retypes free text only (a note cannot be retyped in the core)', () => {
    expect(canRetext(freeText)).toBe(true);
    expect([highlight, note, stamp, ink].some(canRetext)).toBe(false);
  });

  it('keeps a stamp image in proportion', () => {
    expect(keepsProportions(stamp)).toBe(true);
    expect(keepsProportions(shape)).toBe(false);
  });
});

describe('resizeFloor', () => {
  it('is just over the core minimum of a text box (the snapshot is 32-bit; sideways on a quarter-turned page), and a few points otherwise', () => {
    expect(resizeFloor(freeText, 0)).toEqual({ width: expect.closeTo(16.01), height: expect.closeTo(17.81) });
    expect(resizeFloor(freeText, 90)).toEqual({ width: expect.closeTo(17.81), height: expect.closeTo(16.01) });
    expect(resizeFloor(shape, 0)).toEqual({ width: MIN_RESIZE_PT, height: MIN_RESIZE_PT });
  });
});

describe('a box already under the floor', () => {
  it('may stay as it is: a side left alone never grows', () => {
    const thin = { ...highlight, rect: { ...box, height: 2 } } as Annotation;
    expect(resizeFloor(thin, 0)).toEqual({ width: MIN_RESIZE_PT, height: 2 });
  });
});

describe('clampMove', () => {
  it('keeps the move as asked while the box stays on the page', () => {
    expect(clampMove(box, 12, -12, LETTER)).toEqual({ dx: 12, dy: -12 });
  });

  it('stops the box at the page edges', () => {
    expect(clampMove(box, -500, 0, LETTER)).toEqual({ dx: -100, dy: 0 });
    expect(clampMove(box, 1000, 1000, LETTER)).toEqual({ dx: 612 - 180, dy: 792 - 240 });
  });
});

describe('cornerPoint', () => {
  it('names each corner of a box', () => {
    expect(cornerPoint(box, 'nw')).toEqual({ x: 100, y: 200 });
    expect(cornerPoint(box, 'se')).toEqual({ x: 180, y: 240 });
    expect(cornerPoint(box, 'ne')).toEqual({ x: 180, y: 200 });
    expect(cornerPoint(box, 'sw')).toEqual({ x: 100, y: 240 });
  });
});

describe('resizedRect', () => {
  const floor = { width: MIN_RESIZE_PT, height: MIN_RESIZE_PT };

  it('follows the dragged corner and keeps the opposite one', () => {
    expect(resizedRect(box, 'se', { x: 220, y: 300 }, floor, LETTER, false)).toEqual({ left: 100, top: 200, width: 120, height: 100 });
    expect(resizedRect(box, 'nw', { x: 60, y: 180 }, floor, LETTER, false)).toEqual({ left: 60, top: 180, width: 120, height: 60 });
  });

  it('never turns the box inside out: it stops at the floor', () => {
    expect(resizedRect(box, 'se', { x: 50, y: 100 }, floor, LETTER, false)).toEqual({ left: 100, top: 200, width: 4, height: 4 });
    expect(resizedRect(box, 'nw', { x: 500, y: 500 }, floor, LETTER, false)).toEqual({ left: 176, top: 236, width: 4, height: 4 });
  });

  it('stops at the page edges', () => {
    expect(resizedRect(box, 'se', { x: 900, y: 900 }, floor, LETTER, false)).toEqual({ left: 100, top: 200, width: 512, height: 592 });
  });

  it('keeps the proportions when asked, by the larger pull', () => {
    // 80 × 40 pulled to 160 wide (×2) and 50 high (×1.25): ×2 wins.
    expect(resizedRect(box, 'se', { x: 260, y: 250 }, floor, LETTER, true)).toEqual({ left: 100, top: 200, width: 160, height: 80 });
    // Pulled up and left from the bottom-right corner.
    expect(resizedRect(box, 'nw', { x: 20, y: 200 }, floor, LETTER, true)).toEqual({ left: 20, top: 160, width: 160, height: 80 });
  });

  it('keeps the proportions at the page edge too', () => {
    const wide = { left: 500, top: 100, width: 80, height: 40 };
    const grown = resizedRect(wide, 'se', { x: 900, y: 900 }, floor, LETTER, true);
    expect(grown).toEqual({ left: 500, top: 100, width: 112, height: 56 });
  });
});

describe('resizedBy', () => {
  const floor = { width: MIN_RESIZE_PT, height: MIN_RESIZE_PT };

  it('grows or shrinks one side and keeps the top-left corner', () => {
    expect(resizedBy(box, 'se', 12, 0, floor, LETTER, false)).toEqual({ left: 100, top: 200, width: 92, height: 40 });
    expect(resizedBy(box, 'se', 0, -1, floor, LETTER, false)).toEqual({ left: 100, top: 200, width: 80, height: 39 });
  });

  it('anchors the opposite corner, whichever corner is moved', () => {
    expect(resizedBy(box, 'nw', 10, 5, floor, LETTER, false)).toEqual({ left: 90, top: 195, width: 90, height: 45 });
    expect(resizedBy(box, 'ne', 0, 500, floor, LETTER, false)).toEqual({ left: 100, top: 0, width: 80, height: 240 });
    expect(resizedBy(box, 'sw', -20, 0, floor, LETTER, true)).toEqual({ left: 120, top: 200, width: 60, height: 30 });
  });

  it('scales both sides when keeping the proportions', () => {
    expect(resizedBy(box, 'se', -40, 0, floor, LETTER, true)).toEqual({ left: 100, top: 200, width: 40, height: 20 });
    expect(resizedBy(box, 'se', 0, 20, floor, LETTER, true)).toEqual({ left: 100, top: 200, width: 120, height: 60 });
  });

  it('stops at the floor and at the page edge', () => {
    expect(resizedBy(box, 'se', -200, 0, floor, LETTER, false).width).toBe(4);
    expect(resizedBy(box, 'se', 1000, 0, floor, LETTER, false).width).toBe(512);
  });
});

describe('previewEdit', () => {
  it('shows a move on a boxed annotation and on every point of a stroke', () => {
    expect(previewEdit(shape, { kind: 'moveAnnotation', id: 3, dx: 5, dy: -5 })).toEqual({
      ...shape,
      rect: { ...box, left: 105, top: 195 },
    });
    const moved = previewEdit(ink, { kind: 'moveAnnotation', id: 4, dx: 1, dy: 2 });
    expect(moved.kind === 'ink' && moved.points[0]).toEqual({ x: 11, y: 52 });
  });

  it('shows a resize, and leaves what does not move as it is', () => {
    const rect = { left: 1, top: 2, width: 30, height: 40 };
    expect(previewEdit(stamp, { kind: 'resizeAnnotation', id: 6, rect })).toEqual({ ...stamp, rect });
    expect(previewEdit(shape, { kind: 'removeAnnotation', id: 3 })).toBe(shape);
  });
});

describe('sameGeometry', () => {
  it('compares where an annotation is, not its colour or text', () => {
    expect(sameGeometry(shape, { ...shape, color: { r: 0, g: 0, b: 0 } } as Annotation)).toBe(true);
    expect(sameGeometry(shape, { ...shape, rect: { ...box, left: 100.005 } } as Annotation)).toBe(true);
    expect(sameGeometry(shape, { ...shape, rect: { ...box, left: 101 } } as Annotation)).toBe(false);
    const shifted = previewEdit(ink, { kind: 'moveAnnotation', id: 4, dx: 1, dy: 0 });
    expect(sameGeometry(ink, shifted)).toBe(false);
  });
});

describe('annotationLabel', () => {
  it('names the kind and the page, 1-based', () => {
    expect(annotationLabel(freeText)).toBe('Text box on page 1');
    expect(annotationLabel({ ...stamp, pageIndex: 2 })).toBe('Image stamp on page 3');
  });
});
