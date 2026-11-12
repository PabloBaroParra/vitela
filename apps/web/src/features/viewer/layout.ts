// Where every page sits in the scrolling viewer, in CSS pixels. Pure: the
// viewer component and the zoom math both build on it, and both are tested
// without a browser.

import type { PageSize } from '../../core/types';

/** 100% zoom shows a page at its physical size: 96 CSS px per inch. */
export const CSS_PX_PER_PT = 96 / 72;
/** Space between two pages, in CSS px (not scaled by zoom). */
export const PAGE_GAP = 16;
/** Space around the stack of pages, in CSS px (not scaled by zoom). */
export const VIEWER_PADDING = 16;

export interface Rect {
  readonly top: number;
  readonly left: number;
  readonly width: number;
  readonly height: number;
}

export interface Point {
  readonly x: number;
  readonly y: number;
}

export interface Layout {
  /** One slot per page, in page order, in content coordinates. */
  readonly slots: readonly Rect[];
  /** Size of the scrollable content. */
  readonly width: number;
  readonly height: number;
}

/**
 * A place on a page that survives a zoom change: the page and where on it,
 * as fractions of its width and height (outside 0..1 in a gap or margin).
 */
export interface Anchor {
  readonly page: number;
  readonly x: number;
  readonly y: number;
}

/**
 * Stacks the pages vertically, each centred horizontally. The core reports
 * sizes already turned by each page's rotation, so they are used as given.
 */
export function layoutPages(pages: readonly PageSize[], zoom: number, viewportWidth: number): Layout {
  const scale = CSS_PX_PER_PT * zoom;
  const widest = pages.reduce((max, page) => Math.max(max, page.widthPt * scale), 0);
  const width = Math.max(viewportWidth, widest + 2 * VIEWER_PADDING);
  let top = VIEWER_PADDING;
  const slots = pages.map((page) => {
    const slot = { top, left: (width - page.widthPt * scale) / 2, width: page.widthPt * scale, height: page.heightPt * scale };
    top += slot.height + PAGE_GAP;
    return slot;
  });
  const height = slots.length === 0 ? 2 * VIEWER_PADDING : top - PAGE_GAP + VIEWER_PADDING;
  return { slots, width, height };
}

/**
 * The page whose slot holds content `y`; a gap or the padding counts as the
 * page above it (the first page for the top padding). `null` with no pages.
 */
export function pageAt(layout: Layout, y: number): number | null {
  const { slots } = layout;
  if (slots.length === 0) return null;
  // Last slot whose top is at or above y.
  let low = 0;
  let high = slots.length - 1;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (slots[middle]!.top <= y) low = middle;
    else high = middle - 1;
  }
  return low;
}

/**
 * The pages that intersect the band `[top - overscan, top + height + overscan]`,
 * as an inclusive range; `null` with no pages.
 */
export function visiblePages(
  layout: Layout,
  top: number,
  height: number,
  overscan: number,
): { first: number; last: number } | null {
  const first = pageAt(layout, top - overscan);
  const last = pageAt(layout, top + height + overscan);
  if (first === null || last === null) return null;
  // `pageAt` counts a gap as the page above; skip it when only the gap shows.
  const firstSlot = layout.slots[first]!;
  const skip = firstSlot.top + firstSlot.height <= top - overscan && first < last;
  return { first: skip ? first + 1 : first, last };
}

export function anchorAt(layout: Layout, point: Point): Anchor | null {
  const page = pageAt(layout, point.y);
  if (page === null) return null;
  const slot = layout.slots[page]!;
  return { page, x: (point.x - slot.left) / slot.width, y: (point.y - slot.top) / slot.height };
}

export function positionOf(layout: Layout, anchor: Anchor): Point {
  const slot = layout.slots[anchor.page]!;
  return { x: slot.left + anchor.x * slot.width, y: slot.top + anchor.y * slot.height };
}
