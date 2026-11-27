import { describe, expect, it } from 'vitest';
import type { ContentPath, PageContent } from '@vitela/core';
import { hits } from './contentModel';
import { IMAGE_HANDLES } from './images';
import { followPath, MIN_PATH_PT, pathHandles, resizedPathRect } from './paths';

const path = (id: number, rect: ContentPath['rect'], paint: ContentPath['paint'] = 'fill'): ContentPath => ({
  kind: 'path',
  id,
  pageIndex: 0,
  rect,
  paint,
  pending: false,
});
const shape = path(1, { left: 100, top: 200, width: 200, height: 100 });
const rule = path(2, { left: 72, top: 392, width: 200, height: 0 }, 'stroke');
const post = path(3, { left: 400, top: 100, width: 0, height: 300 }, 'stroke');

describe('pathHandles', () => {
  it('gives a shape its eight handles, a rule only the two at its ends', () => {
    expect(pathHandles(shape.rect)).toEqual(IMAGE_HANDLES);
    expect(pathHandles(rule.rect)).toEqual(['e', 'w']);
    expect(pathHandles(post.rect)).toEqual(['n', 's']);
    expect(pathHandles({ left: 1, top: 1, width: 0, height: 0 })).toEqual([]);
  });
});

describe('resizedPathRect', () => {
  it('resizes a corner freely, and in proportion with Shift', () => {
    expect(resizedPathRect(shape.rect, 'se', 100, 10, false)).toEqual({ left: 100, top: 200, width: 300, height: 110 });
    expect(resizedPathRect(shape.rect, 'se', 100, 10, true)).toEqual({ left: 100, top: 200, width: 300, height: 150 });
  });

  it('keeps the opposite side where it is and never goes below the smallest side', () => {
    expect(resizedPathRect(shape.rect, 'w', 50, 0, false)).toEqual({ left: 150, top: 200, width: 150, height: 100 });
    expect(resizedPathRect(shape.rect, 'n', 0, 500, false).height).toBeGreaterThan(0);
  });

  it('lengthens a rule and leaves it flat where it is, whatever the pointer does across it', () => {
    expect(resizedPathRect(rule.rect, 'e', 50, 30, false)).toEqual({ left: 72, top: 392, width: 250, height: 0 });
    expect(resizedPathRect(rule.rect, 'w', -20, -30, true)).toEqual({ left: 52, top: 392, width: 220, height: 0 });
    expect(resizedPathRect(post.rect, 's', 40, 60, false)).toEqual({ left: 400, top: 100, width: 0, height: 360 });
  });

  const bar = { left: 100, top: 300, width: 200, height: 0.5 };

  it('keeps a thin bar thin: the image minimum does not apply to a path', () => {
    expect(resizedPathRect(bar, 'e', 50, 20, false)).toEqual({ left: 100, top: 300, width: 250, height: 0.5 });
    expect(resizedPathRect(bar, 'w', -20, -20, true)).toEqual({ left: 80, top: 300, width: 220, height: 0.5 });
  });

  it('scales a thin bar with Shift from the side that has extent, not inflated', () => {
    const along = resizedPathRect(bar, 'se', 40, 10, true);
    expect(along.width).toBeCloseTo(240);
    expect(along.height).toBeCloseTo(0.6);
    const across = resizedPathRect(bar, 'se', 5, 30, true);
    expect(across.width).toBeCloseTo(205);
    expect(across.height).toBeCloseTo(0.5125);
    expect(across.top).toBe(300);
  });

  it('never changes a side the handle does not move', () => {
    expect(resizedPathRect(shape.rect, 'n', 25, -30, false)).toEqual({ left: 100, top: 170, width: 200, height: 130 });
    expect(resizedPathRect(shape.rect, 'e', 40, 999, false)).toEqual({ left: 100, top: 200, width: 240, height: 100 });
  });

  it('cannot resize a side to nothing, nor grow a side already below the smallest', () => {
    expect(resizedPathRect(shape.rect, 'e', -500, 0, false).width).toBe(MIN_PATH_PT);
    expect(resizedPathRect(shape.rect, 'n', 0, 500, false)).toEqual({ left: 100, top: 300 - MIN_PATH_PT, width: 200, height: MIN_PATH_PT });
    expect(resizedPathRect(shape.rect, 'se', -500, -500, true).width).toBeGreaterThan(0);
    const sliver = { left: 10, top: 10, width: 0.5, height: 100 };
    expect(resizedPathRect(sliver, 'e', -10, 0, false)).toEqual(sliver);
  });
});

describe('followPath', () => {
  const content: PageContent = { runs: [], images: [], paths: [shape, rule, { ...shape, id: 4, rect: { ...shape.rect, left: 108 } }] };

  it('finds the path closest to where it should be now, a rule of no height included', () => {
    expect(followPath(content, { ...shape.rect, left: 107 })?.id).toBe(4);
    expect(followPath(content, rule.rect)?.id).toBe(2);
  });

  it('finds nothing for a removed path or where none overlaps', () => {
    expect(followPath(content, null)).toBeNull();
    expect(followPath(content, { left: 600, top: 700, width: 10, height: 10 })).toBeNull();
  });
});

describe('hits for a path', () => {
  const frame = path(5, { left: 100, top: 100, width: 200, height: 100 }, 'stroke');

  it('takes an outline only near its edges: further in is its cell, not its border', () => {
    expect(hits(frame, { x: 101, y: 150 })).toBe(true);
    expect(hits(frame, { x: 200, y: 98 })).toBe(true);
    expect(hits(frame, { x: 200, y: 150 })).toBe(false);
  });

  it('takes a filled shape anywhere inside it', () => {
    expect(hits({ ...frame, paint: 'fill' }, { x: 200, y: 150 })).toBe(true);
    expect(hits({ ...frame, paint: 'fillAndStroke' }, { x: 200, y: 150 })).toBe(true);
  });
});
