import { describe, expect, it } from 'vitest';
import type { PageSize } from '@vitela/core';
import { toPagePoint } from '../text/pageRects';
import { freeTextFrame } from './geometry';
import {
  clickRect,
  drawnEdit,
  drawnSize,
  extendStroke,
  finishGesture,
  fitOnPage,
  FREE_TEXT_CLICK_SIZE,
  FREE_TEXT_MIN_SIZE,
  isBlank,
  isDrag,
  NOTE_SIZE,
  PALETTE,
  SHAPE_CLICK_SIZE,
  textEdit,
  tracedRect,
  usesColor,
} from './drawing';

const LETTER: PageSize = { widthPt: 612, heightPt: 792, rotationDegrees: 0 };
/** A Letter page with `/Rotate 90`: drawn landscape. */
const TURNED: PageSize = { widthPt: 792, heightPt: 612, rotationDegrees: 90 };
const YELLOW = { r: 255, g: 220, b: 0 };

describe('drawing geometry', () => {
  it('tells a drag from a click by distance from the press', () => {
    expect(isDrag({ x: 10, y: 10 }, { x: 15, y: 15 })).toBe(false); // ~7 pt
    expect(isDrag({ x: 10, y: 10 }, { x: 16, y: 16 })).toBe(true); // ~8.5 pt
    expect(isDrag({ x: 10, y: 10 }, { x: 10, y: 18 })).toBe(true);
  });

  it('traces the same box whichever way the drag goes', () => {
    const rect = { left: 100, top: 200, width: 50, height: 30 };
    expect(tracedRect({ x: 100, y: 200 }, { x: 150, y: 230 }, { width: 4, height: 4 }, LETTER)).toEqual(rect);
    expect(tracedRect({ x: 150, y: 230 }, { x: 100, y: 200 }, { width: 4, height: 4 }, LETTER)).toEqual(rect);
    expect(tracedRect({ x: 150, y: 200 }, { x: 100, y: 230 }, { width: 4, height: 4 }, LETTER)).toEqual(rect);
  });

  it('floors a flat drag away from the press and keeps it on the page', () => {
    // A flat sweep to the left, drifting down: the floor grows the box downward from the press.
    expect(tracedRect({ x: 100, y: 300 }, { x: 40, y: 302 }, FREE_TEXT_MIN_SIZE, LETTER)).toEqual({
      left: 40,
      top: 300,
      width: 60,
      height: FREE_TEXT_MIN_SIZE.height,
    });
    // Drifting up next to the top edge: the floor would cross it, so the box slides back on.
    expect(tracedRect({ x: 100, y: 5 }, { x: 160, y: 4 }, FREE_TEXT_MIN_SIZE, LETTER).top).toBe(0);
    // Dragged off the page: cut at its edge.
    expect(tracedRect({ x: 600, y: 780 }, { x: 900, y: 900 }, { width: 4, height: 4 }, LETTER)).toEqual({
      left: 600,
      top: 780,
      width: 12,
      height: 12,
    });
  });

  it('places a click box at the press, slid back onto the page near an edge', () => {
    expect(clickRect({ x: 72, y: 72 }, FREE_TEXT_CLICK_SIZE, LETTER)).toEqual({ left: 72, top: 72, width: 200, height: 50 });
    expect(clickRect({ x: 600, y: 780 }, FREE_TEXT_CLICK_SIZE, LETTER)).toEqual({ left: 412, top: 742, width: 200, height: 50 });
    expect(fitOnPage({ left: -5, top: 10, width: 1000, height: 10 }, LETTER)).toEqual({ left: 0, top: 10, width: 612, height: 10 });
  });

  it('works in the turned page as drawn: its own width and height bound the box', () => {
    // 700 pt across is inside a landscape-drawn page, past a portrait one.
    expect(clickRect({ x: 700, y: 100 }, NOTE_SIZE, TURNED)).toEqual({ left: 700, top: 100, width: 24, height: 24 });
    expect(clickRect({ x: 700, y: 600 }, NOTE_SIZE, TURNED)).toEqual({ left: 700, top: 588, width: 24, height: 24 });
    expect(clickRect({ x: 700, y: 100 }, NOTE_SIZE, LETTER).left).toBe(588);
  });

  it('turns pointer offsets into drawn points at any zoom', () => {
    // The same spot on the page at 50 %, 100 % and 300 % (CSS px per point = 4/3 × zoom).
    for (const zoom of [0.5, 1, 3]) {
      const scale = (96 / 72) * zoom;
      const start = toPagePoint(100 * scale, 200 * scale, scale);
      const end = toPagePoint(150 * scale, 260 * scale, scale);
      const gesture = finishGesture('shape', [start, end], TURNED);
      if (gesture?.kind !== 'box') throw new Error('no box');
      const { left, top, width, height } = gesture.rect;
      expect([left, top, width, height].map((value) => Math.round(value * 1e6) / 1e6)).toEqual([100, 200, 50, 60]);
    }
  });

  it('keeps a stroke on the page and drops points that add nothing', () => {
    let points = extendStroke([], { x: 10, y: 10 }, LETTER);
    points = extendStroke(points, { x: 10.2, y: 10.1 }, LETTER);
    points = extendStroke(points, { x: 20, y: -5 }, LETTER);
    points = extendStroke(points, { x: 900, y: 30 }, LETTER);
    expect(points).toEqual([
      { x: 10, y: 10 },
      { x: 20, y: 0 },
      { x: 612, y: 30 },
    ]);
  });
});

describe('finishGesture', () => {
  const press = { x: 100, y: 100 };
  const click = [press, { x: 102, y: 101 }];
  const drag = [press, { x: 130, y: 120 }, { x: 180, y: 160 }];

  it('ink keeps a stroke of two points or more', () => {
    expect(finishGesture('ink', [press], LETTER)).toBeNull();
    expect(finishGesture('ink', drag, LETTER)).toEqual({ kind: 'stroke', points: drag });
    expect(finishGesture('ink', [], LETTER)).toBeNull();
  });

  it('a shape is the traced box, or the default box for a click', () => {
    expect(finishGesture('shape', drag, LETTER)).toEqual({ kind: 'box', rect: { left: 100, top: 100, width: 80, height: 60 } });
    expect(finishGesture('shape', click, LETTER)).toEqual({
      kind: 'box',
      rect: { left: 100, top: 100, ...SHAPE_CLICK_SIZE },
    });
  });

  it('a note is its icon box at the press, dragged or not', () => {
    const rect = { left: 100, top: 100, ...NOTE_SIZE };
    expect(finishGesture('note', click, LETTER)).toEqual({ kind: 'box', rect });
    expect(finishGesture('note', drag, LETTER)).toEqual({ kind: 'box', rect });
  });

  it('a text box is the traced box at least one line high, or the default box for a click', () => {
    expect(finishGesture('freeText', drag, LETTER)).toEqual({ kind: 'box', rect: { left: 100, top: 100, width: 80, height: 60 } });
    expect(finishGesture('freeText', [press, { x: 200, y: 102 }], LETTER)).toEqual({
      kind: 'box',
      rect: { left: 100, top: 100, width: 100, height: FREE_TEXT_MIN_SIZE.height },
    });
    expect(finishGesture('freeText', click, LETTER)).toEqual({
      kind: 'box',
      rect: { left: 100, top: 100, ...FREE_TEXT_CLICK_SIZE },
    });
  });
});

describe('edits', () => {
  it('a stroke is one addInk and a box one addShape, in the chosen colour', () => {
    const points = [
      { x: 1, y: 2 },
      { x: 3, y: 4 },
    ];
    const rect = { left: 1, top: 2, width: 3, height: 4 };
    expect(drawnEdit('ink', 2, { kind: 'stroke', points }, YELLOW)).toEqual({ kind: 'addInk', page: 2, points, color: YELLOW });
    expect(drawnEdit('shape', 0, { kind: 'box', rect }, YELLOW)).toEqual({ kind: 'addShape', page: 0, rect, color: YELLOW });
    expect(drawnEdit('ink', 0, { kind: 'box', rect }, YELLOW)).toBeNull();
  });

  it('a note and a text box carry their text', () => {
    const rect = { left: 1, top: 2, width: 3, height: 4 };
    expect(textEdit('note', 1, rect, 'hi')).toEqual({ kind: 'addNote', page: 1, rect, contents: 'hi' });
    expect(textEdit('freeText', 1, rect, 'hi')).toEqual({ kind: 'addFreeText', page: 1, rect, contents: 'hi' });
  });

  it('blank text is refused; colour is only for ink and shapes; the palette starts at the core default', () => {
    expect(isBlank(' \n\t')).toBe(true);
    expect(isBlank(' a ')).toBe(false);
    expect(['select', 'ink', 'shape', 'note', 'freeText'].map((tool) => usesColor(tool as never))).toEqual([
      false,
      true,
      true,
      false,
      false,
    ]);
    expect(PALETTE[0]!.color).toEqual(YELLOW);
  });
});

describe('rotation-aware text box sizes', () => {
  const frame = { width: 200, height: 50 };

  it('a text frame size is the drawn size upright and upside down, swapped on a quarter turn', () => {
    expect(drawnSize(frame, 0)).toEqual({ width: 200, height: 50 });
    expect(drawnSize(frame, 90)).toEqual({ width: 50, height: 200 });
    expect(drawnSize(frame, 180)).toEqual({ width: 200, height: 50 });
    expect(drawnSize(frame, 270)).toEqual({ width: 50, height: 200 });
    expect(drawnSize(frame, -90)).toEqual({ width: 50, height: 200 });
  });

  const press = { x: 100, y: 100 };
  const click = [press, { x: 102, y: 101 }];
  for (const rotationDegrees of [0, 90, 180, 270]) {
    const sideways = rotationDegrees % 180 !== 0;
    const page: PageSize = sideways ? { widthPt: 792, heightPt: 612, rotationDegrees } : { ...LETTER, rotationDegrees };

    it(`a click at ${rotationDegrees}° gives a text frame of the default size`, () => {
      const gesture = finishGesture('freeText', click, page);
      expect(gesture?.kind).toBe('box');
      const rect = gesture!.kind === 'box' ? gesture!.rect : null;
      const { width, height } = freeTextFrame(rect!, rotationDegrees);
      expect({ width, height }).toEqual(FREE_TEXT_CLICK_SIZE);
    });

    it(`a flat drag at ${rotationDegrees}° still gives a text frame one line high and one glyph wide`, () => {
      // A sweep along the drawn x axis, and one along the drawn y axis.
      for (const end of [{ x: 200, y: 101 }, { x: 101, y: 200 }]) {
        const gesture = finishGesture('freeText', [press, end], page);
        const rect = gesture!.kind === 'box' ? gesture!.rect : null;
        const { width, height } = freeTextFrame(rect!, rotationDegrees);
        expect(width).toBeGreaterThanOrEqual(FREE_TEXT_MIN_SIZE.width);
        expect(height).toBeGreaterThanOrEqual(FREE_TEXT_MIN_SIZE.height);
      }
    });
  }

  it('on a page turned 90° the minimum is laid sideways in drawn space', () => {
    expect(finishGesture('freeText', [press, { x: 200, y: 102 }], TURNED)).toEqual({
      kind: 'box',
      rect: { left: 100, top: 100, width: 100, height: FREE_TEXT_MIN_SIZE.width },
    });
    expect(finishGesture('freeText', [press, { x: 102, y: 200 }], TURNED)).toEqual({
      kind: 'box',
      rect: { left: 100, top: 100, width: FREE_TEXT_MIN_SIZE.height, height: 100 },
    });
    expect(finishGesture('freeText', click, TURNED)).toEqual({
      kind: 'box',
      rect: { left: 100, top: 100, width: FREE_TEXT_CLICK_SIZE.height, height: FREE_TEXT_CLICK_SIZE.width },
    });
  });
});
