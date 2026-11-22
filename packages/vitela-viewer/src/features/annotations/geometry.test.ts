import { describe, expect, it } from 'vitest';
import type { MarkupKind } from '@vitela/core';
import { colorCss, freeTextFrame, markupEdits, markupRule, RULE_PT } from './geometry';

const RECT = { left: 100, top: 200, width: 60, height: 20 };
const HALF = RULE_PT / 2;

describe('markupRule', () => {
  it('underlines along the drawn bottom edge of an upright page, inside the box', () => {
    expect(markupRule('underline', RECT, 0)).toEqual({ x1: 100, y1: 220 - HALF, x2: 160, y2: 220 - HALF });
  });

  it('strikes through the middle of an upright page', () => {
    expect(markupRule('strikeout', RECT, 0)).toEqual({ x1: 100, y1: 210, x2: 160, y2: 210 });
  });

  // A quarter turn clockwise takes the PDF's bottom edge to the drawn left,
  // and the text runs down the page: the rules turn with it.
  it('follows the PDF bottom edge to the left side on a page turned 90°', () => {
    const rect = { left: 100, top: 200, width: 20, height: 60 };
    expect(markupRule('underline', rect, 90)).toEqual({ x1: 100 + HALF, y1: 200, x2: 100 + HALF, y2: 260 });
    expect(markupRule('strikeout', rect, 90)).toEqual({ x1: 110, y1: 200, x2: 110, y2: 260 });
  });

  it('puts the PDF bottom edge on top at 180° and on the right at 270°', () => {
    expect(markupRule('underline', RECT, 180)).toEqual({ x1: 100, y1: 200 + HALF, x2: 160, y2: 200 + HALF });
    const rect = { left: 100, top: 200, width: 20, height: 60 };
    expect(markupRule('underline', rect, 270)).toEqual({ x1: 120 - HALF, y1: 200, x2: 120 - HALF, y2: 260 });
  });

  it('treats a turn past a full circle, or a negative one, as the same turn', () => {
    expect(markupRule('underline', RECT, 360)).toEqual(markupRule('underline', RECT, 0));
    expect(markupRule('underline', RECT, -90)).toEqual(markupRule('underline', RECT, 270));
  });
});

describe('freeTextFrame', () => {
  it('is the box itself on an upright page', () => {
    expect(freeTextFrame(RECT, 0)).toEqual({ width: 60, height: 20, transform: 'translate(130 210) rotate(0) translate(-30 -10)' });
  });

  it('swaps the sides and turns with a page turned a quarter', () => {
    // Drawn 20 wide and 60 tall: the box's own frame is 60 by 20, turned 90°.
    const rect = { left: 100, top: 200, width: 20, height: 60 };
    expect(freeTextFrame(rect, 90)).toEqual({ width: 60, height: 20, transform: 'translate(110 230) rotate(90) translate(-30 -10)' });
  });
});

describe('markupEdits', () => {
  const yellow = { r: 255, g: 220, b: 0 };

  it('makes one edit per selected line, in order', () => {
    const rects = new Float32Array([72, 100, 200, 18, 72, 130, 150, 18]);
    const kind: MarkupKind = 'highlight';
    expect(markupEdits(3, rects, kind, yellow)).toEqual([
      { kind: 'addMarkup', page: 3, markup: 'highlight', rect: { left: 72, top: 100, width: 200, height: 18 }, color: yellow },
      { kind: 'addMarkup', page: 3, markup: 'highlight', rect: { left: 72, top: 130, width: 150, height: 18 }, color: yellow },
    ]);
  });

  it('skips empty boxes and a trailing partial one', () => {
    const rects = new Float32Array([72, 100, 0, 18, 72, 130, 150, 0, 1, 2, 3]);
    expect(markupEdits(0, rects, 'underline', yellow)).toEqual([]);
  });
});

describe('colorCss', () => {
  it('writes an rgb() colour', () => {
    expect(colorCss({ r: 255, g: 220, b: 0 })).toBe('rgb(255 220 0)');
  });
});
