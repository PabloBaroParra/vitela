import { describe, expect, it } from 'vitest';
import type { ContentRun, PageContent, TextRunStyle } from '@vitela/core';
import {
  DRAG_THRESHOLD_PX,
  NUDGE_PT,
  NUDGE_SHIFT_PT,
  editorBox,
  fallbackFamilies,
  faceOf,
  followRun,
  isDrag,
  nudgeOf,
  retypeOf,
  toggled,
} from './textRuns';

const run = (id: number, left: number, top: number, text: string, width = 60): ContentRun => ({
  kind: 'run',
  id,
  pageIndex: 0,
  rect: { left, top, width, height: 12 },
  fontName: 'F1',
  fontKind: 'standard14',
  text,
  editable: true,
});
const page = (...runs: ContentRun[]): PageContent => ({ runs, images: [], paths: [] });

describe('nudgeOf', () => {
  it('moves by a point per arrow, ten with Shift, and ignores every other key', () => {
    expect(nudgeOf('ArrowLeft', false)).toEqual({ dx: -NUDGE_PT, dy: 0 });
    expect(nudgeOf('ArrowRight', false)).toEqual({ dx: NUDGE_PT, dy: 0 });
    expect(nudgeOf('ArrowUp', true)).toEqual({ dx: 0, dy: -NUDGE_SHIFT_PT });
    expect(nudgeOf('ArrowDown', true)).toEqual({ dx: 0, dy: NUDGE_SHIFT_PT });
    expect(nudgeOf('a', false)).toBeNull();
    expect(NUDGE_SHIFT_PT).toBe(10 * NUDGE_PT);
  });
});

describe('isDrag', () => {
  it('takes a press that travels past the threshold for a drag, and a shorter one for a click', () => {
    expect(isDrag(0, DRAG_THRESHOLD_PX - 1)).toBe(false);
    expect(isDrag(DRAG_THRESHOLD_PX, 0)).toBe(true);
    expect(isDrag(-3, -3)).toBe(true);
  });
});

describe('retypeOf', () => {
  it('retypes, removes when cleared, and does nothing for the same text', () => {
    const hello = run(4, 72, 72, 'Hello');
    expect(retypeOf(hello, 'Howdy')).toEqual({ kind: 'retypeTextRun', page: 0, runId: 4, text: 'Howdy' });
    expect(retypeOf(hello, '')).toEqual({ kind: 'removeTextRun', page: 0, runId: 4 });
    expect(retypeOf(hello, 'Hello')).toBeNull();
  });
});

describe('toggled', () => {
  it('turns one face on or off and keeps the rest', () => {
    const look = { bold: false, italic: true, underline: false, strike: false };
    expect(toggled(look, 'bold')).toEqual({ bold: true, italic: true, underline: false, strike: false });
    expect(toggled(look, 'italic')).toEqual({ bold: false, italic: false, underline: false, strike: false });
  });
});

describe('followRun', () => {
  it('finds the edited run again by its text where it was expected', () => {
    const content = page(run(0, 72, 72, 'Other'), run(1, 72, 100, 'Howdy'));
    expect(followRun(content, { left: 72, top: 100, width: 60, height: 12 }, 'Howdy')?.id).toBe(1);
  });

  it('finds a moved run at its new place, the one overlapping most', () => {
    const content = page(run(0, 72, 100, 'Hello'), run(1, 102, 100, 'Hello'));
    expect(followRun(content, { left: 100, top: 100, width: 60, height: 12 }, 'Hello')?.id).toBe(1);
  });

  it('finds nothing where nothing overlaps, or when the text was removed', () => {
    const content = page(run(0, 72, 72, 'Hello'));
    expect(followRun(content, { left: 400, top: 400, width: 60, height: 12 }, 'Hello')).toBeNull();
    expect(followRun(content, { left: 72, top: 72, width: 60, height: 12 }, null)).toBeNull();
  });

  it('settles for another text at the place when the run reads differently now', () => {
    // A retype the core wrote into a fallback face reads back split.
    const content = page(run(3, 72, 72, 'Howd'));
    expect(followRun(content, { left: 72, top: 72, width: 60, height: 12 }, 'Howdy')?.id).toBe(3);
  });
});

describe('fallbackFamilies', () => {
  it('names the family without its subset tag, then a generic', () => {
    expect(fallbackFamilies('ABCDEF+Vera-Bold')).toEqual(['Vera', 'sans-serif']);
    expect(fallbackFamilies('Times-Roman')).toEqual(['Times', 'Times New Roman', 'serif']);
    expect(fallbackFamilies('Courier')).toEqual(['Courier', 'Courier New', 'monospace']);
    expect(fallbackFamilies('Helvetica')).toEqual(['Helvetica', 'Arial', 'sans-serif']);
    expect(fallbackFamilies('')).toEqual(['sans-serif']);
  });
});

describe('faceOf', () => {
  const style: TextRunStyle = {
    baseFont: 'ABCDEF+Arial,BoldItalic',
    fontSizePt: 12,
    color: { r: 0, g: 0, b: 0 },
    charSpacingPt: 0,
    horizontalScale: 1,
    bold: false,
    italic: false,
    renderMode: 0,
  };

  it('reads weight and slant from the look, the style and the name, and a stroked mode as bold', () => {
    expect(faceOf(style, null)).toEqual({ bold: true, italic: true });
    const plain = { ...style, baseFont: 'Helvetica' };
    expect(faceOf(plain, null)).toEqual({ bold: false, italic: false });
    expect(faceOf({ ...plain, renderMode: 2 }, null)).toEqual({ bold: true, italic: false });
    expect(faceOf(plain, { bold: false, italic: true, underline: false, strike: false })).toEqual({ bold: false, italic: true });
  });
});

describe('editorBox', () => {
  const rect = { left: 10, top: 20, width: 100, height: 12 };

  it('covers the run on an upright page', () => {
    expect(editorBox(rect, 0, 2)).toEqual({ left: 20, top: 40, width: 200, height: 24, rotation: 0 });
  });

  it('turns with the page, laid along the text', () => {
    // On a quarter-turned page the run is drawn tall: the editor is as long
    // as the drawn box is tall, centred on it and turned.
    const drawn = { left: 10, top: 20, width: 12, height: 100 };
    expect(editorBox(drawn, 90, 1)).toEqual({ left: -34, top: 64, width: 100, height: 12, rotation: 90 });
  });
});
