// Between the engine's page geometry (drawn PDF points from the page's
// top-left corner, `/Rotate` already applied by the core) and CSS pixels
// inside a page slot. Pure: the only conversion left to the web shell is
// the zoom.

import type { PagePoint, PageRects } from '../../core/types';
import type { Rect } from '../viewer/layout';

/** Floats per rect in a `PageRects` buffer: left, top, width, height. */
const STRIDE = 4;

/** `rects` at `scale` CSS px per point, relative to the page's top-left corner. */
export function toCssRects(rects: PageRects, scale: number): Rect[] {
  const out: Rect[] = [];
  for (let i = 0; i + STRIDE <= rects.length; i += STRIDE) {
    out.push({
      left: rects[i]! * scale,
      top: rects[i + 1]! * scale,
      width: rects[i + 2]! * scale,
      height: rects[i + 3]! * scale,
    });
  }
  return out;
}

/** A pointer `x, y` CSS px from the page's top-left corner, in drawn points. */
export function toPagePoint(x: number, y: number, scale: number): PagePoint {
  return { x: x / scale, y: y / scale };
}

/** The smallest rect holding every rect of `rects` (CSS px); `null` if empty. */
export function boundingBox(rects: readonly Rect[]): Rect | null {
  if (rects.length === 0) return null;
  const left = Math.min(...rects.map((rect) => rect.left));
  const top = Math.min(...rects.map((rect) => rect.top));
  const right = Math.max(...rects.map((rect) => rect.left + rect.width));
  const bottom = Math.max(...rects.map((rect) => rect.top + rect.height));
  return { left, top, width: right - left, height: bottom - top };
}
