import { describe, expect, it } from 'vitest';
import type { PageSize } from '../../core/types';
import { layoutPages, type Layout } from './layout';
import { MAX_ZOOM, MIN_ZOOM, clampZoom, fitWidthZoom, wheelZoom, zoomAround, zoomIn, zoomOut } from './zoom';

const letter: PageSize = { widthPt: 612, heightPt: 792, rotationDegrees: 0 };

describe('clampZoom', () => {
  it('keeps the zoom inside the supported range', () => {
    expect(clampZoom(0.01)).toBe(MIN_ZOOM);
    expect(clampZoom(100)).toBe(MAX_ZOOM);
    expect(clampZoom(1.3)).toBe(1.3);
  });
});

describe('zoomIn / zoomOut', () => {
  it('steps to the next preset in each direction', () => {
    expect(zoomIn(1)).toBe(1.1);
    expect(zoomOut(1)).toBe(0.9);
  });

  it('snaps a zoom between presets to the nearest preset in the asked direction', () => {
    expect(zoomIn(1.03)).toBe(1.1);
    expect(zoomOut(1.03)).toBe(1);
  });

  it('stops at the ends of the range', () => {
    expect(zoomIn(MAX_ZOOM)).toBe(MAX_ZOOM);
    expect(zoomOut(MIN_ZOOM)).toBe(MIN_ZOOM);
  });
});

describe('wheelZoom', () => {
  it('zooms in when the wheel moves up and out when it moves down', () => {
    expect(wheelZoom(1, -100, 0)).toBeGreaterThan(1);
    expect(wheelZoom(1, 100, 0)).toBeLessThan(1);
  });

  it('is symmetric: up then down by the same amount returns to the start', () => {
    expect(wheelZoom(wheelZoom(1, -100, 0), 100, 0)).toBeCloseTo(1, 10);
  });

  it('treats line and page deltas as larger pixel deltas', () => {
    expect(wheelZoom(1, -3, 1)).toBeCloseTo(wheelZoom(1, -48, 0), 10);
    expect(wheelZoom(1, -1, 2)).toBeGreaterThan(wheelZoom(1, -100, 0));
  });

  it('clamps to the supported range', () => {
    expect(wheelZoom(1, -100_000, 0)).toBe(MAX_ZOOM);
    expect(wheelZoom(1, 100_000, 0)).toBe(MIN_ZOOM);
  });
});

describe('fitWidthZoom', () => {
  it('makes the widest page span the viewport minus padding', () => {
    const pages = [letter, { widthPt: 792, heightPt: 612, rotationDegrees: 90 }];
    const zoom = fitWidthZoom(pages, 1088);
    const widest = layoutPages(pages, zoom, 1088).slots[1]!;
    expect(widest.left).toBeCloseTo(16, 6);
    expect(widest.width).toBeCloseTo(1088 - 32, 6);
  });

  it('falls back to 100% for an empty document', () => {
    expect(fitWidthZoom([], 1000)).toBe(1);
  });
});

describe('zoomAround', () => {
  const pages = Array<PageSize>(5).fill(letter);
  const viewport = { width: 1000, height: 700 };

  /** Where `point` (content coordinates) falls inside page `page`, as fractions. */
  const fraction = (layout: Layout, page: number, point: { x: number; y: number }) => ({
    x: (point.x - layout.slots[page]!.left) / layout.slots[page]!.width,
    y: (point.y - layout.slots[page]!.top) / layout.slots[page]!.height,
  });

  it('keeps the page point under the pointer where it was', () => {
    const before = { zoom: 1, scroll: { x: 0, y: 1500 } };
    const pointer = { x: 400, y: 300 };
    const after = zoomAround(pages, viewport, before, 2, pointer);

    expect(after.zoom).toBe(2);
    const was = fraction(layoutPages(pages, 1, viewport.width), 1, {
      x: before.scroll.x + pointer.x,
      y: before.scroll.y + pointer.y,
    });
    const is = fraction(layoutPages(pages, 2, viewport.width), 1, {
      x: after.scroll.x + pointer.x,
      y: after.scroll.y + pointer.y,
    });
    expect(is.x).toBeCloseTo(was.x, 9);
    expect(is.y).toBeCloseTo(was.y, 9);
  });

  it('never scrolls before the start of the content', () => {
    const after = zoomAround(pages, viewport, { zoom: 2, scroll: { x: 0, y: 0 } }, 0.5, { x: 900, y: 600 });
    expect(after.scroll.x).toBeGreaterThanOrEqual(0);
    expect(after.scroll.y).toBeGreaterThanOrEqual(0);
  });

  it('clamps the requested zoom', () => {
    expect(zoomAround(pages, viewport, { zoom: 1, scroll: { x: 0, y: 0 } }, 99, { x: 0, y: 0 }).zoom).toBe(MAX_ZOOM);
  });

  it('only changes the zoom of an empty document', () => {
    expect(zoomAround([], viewport, { zoom: 1, scroll: { x: 5, y: 7 } }, 2, { x: 1, y: 1 })).toEqual({
      zoom: 2,
      scroll: { x: 5, y: 7 },
    });
  });
});
