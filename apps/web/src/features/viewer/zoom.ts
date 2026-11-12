// Zoom levels and the scroll position that keeps a point still while
// zooming. Pure, so the anchoring math is unit-tested without a browser.

import type { PageSize } from '../../core/types';
import { CSS_PX_PER_PT, VIEWER_PADDING, anchorAt, layoutPages, positionOf, type Point } from './layout';

export const MIN_ZOOM = 0.25;
export const MAX_ZOOM = 4;

/** The levels the +/- buttons step through. */
const PRESETS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4];
/** Ignore float noise when deciding whether a zoom already sits on a preset. */
const EPSILON = 1e-6;

/** How much one pixel of wheel travel zooms: 100 px is about 22%. */
const WHEEL_ZOOM_RATE = 0.002;
/** WheelEvent.deltaMode: 1 = lines, 2 = pages; converted to pixels. */
const LINE_PX = 16;
const PAGE_PX = 800;

export function clampZoom(zoom: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom));
}

export function zoomIn(zoom: number): number {
  return PRESETS.find((preset) => preset > zoom + EPSILON) ?? MAX_ZOOM;
}

export function zoomOut(zoom: number): number {
  return [...PRESETS].reverse().find((preset) => preset < zoom - EPSILON) ?? MIN_ZOOM;
}

/** The zoom after one Ctrl/⌘+wheel event; wheel up (negative delta) zooms in. */
export function wheelZoom(zoom: number, deltaY: number, deltaMode: number): number {
  const pixels = deltaY * (deltaMode === 1 ? LINE_PX : deltaMode === 2 ? PAGE_PX : 1);
  return clampZoom(zoom * Math.exp(-pixels * WHEEL_ZOOM_RATE));
}

/** The zoom at which the widest page spans the viewport, inside the padding. */
export function fitWidthZoom(pages: readonly PageSize[], viewportWidth: number): number {
  const widestPt = pages.reduce((max, page) => Math.max(max, page.widthPt), 0);
  if (widestPt === 0) return 1;
  return clampZoom((viewportWidth - 2 * VIEWER_PADDING) / (widestPt * CSS_PX_PER_PT));
}

export interface ZoomView {
  readonly zoom: number;
  /** scrollLeft / scrollTop of the viewer. */
  readonly scroll: Point;
}

/**
 * Zooms to `zoom` keeping the page point under `pointer` (viewport
 * coordinates) under it: the point is found in the old layout as a fraction
 * of its page, then the scroll is chosen so that fraction of the same page
 * lands under the pointer in the new layout.
 */
export function zoomAround(
  pages: readonly PageSize[],
  viewport: { readonly width: number; readonly height: number },
  view: ZoomView,
  zoom: number,
  pointer: Point,
): ZoomView {
  const next = clampZoom(zoom);
  const before = layoutPages(pages, view.zoom, viewport.width);
  const anchor = anchorAt(before, { x: view.scroll.x + pointer.x, y: view.scroll.y + pointer.y });
  if (anchor === null) return { zoom: next, scroll: view.scroll };
  const target = positionOf(layoutPages(pages, next, viewport.width), anchor);
  return {
    zoom: next,
    // The browser clamps the far end; the near end is clamped here so the
    // value we remember is the one the browser will actually use.
    scroll: { x: Math.max(0, target.x - pointer.x), y: Math.max(0, target.y - pointer.y) },
  };
}
