// The scroll that brings a rect of the content into view. Pure, so jumping
// to a search match is tested without a browser — and works for a page that
// is not mounted: the layout says where every page is.

import type { Point, Rect } from './layout';

/** The scroller's visible part of the content: its scroll and client size. */
export interface Viewport {
  readonly left: number;
  readonly top: number;
  readonly width: number;
  readonly height: number;
}

/**
 * The scroll (`x` = scrollLeft, `y` = scrollTop) that shows `target` with
 * `margin` px around it, or `null` if it already shows. Each axis moves only
 * if the target is not inside it, and then centres the target on that axis
 * (a target bigger than the viewport starts at its near edge). Never
 * negative; the browser clamps the far end.
 */
export function revealScroll(viewport: Viewport, target: Rect, margin: number): Point | null {
  const x = axis(viewport.left, viewport.width, target.left, target.width, margin);
  const y = axis(viewport.top, viewport.height, target.top, target.height, margin);
  if (x === viewport.left && y === viewport.top) return null;
  return { x, y };
}

function axis(scroll: number, size: number, start: number, length: number, margin: number): number {
  const fits = start - margin >= scroll && start + length + margin <= scroll + size;
  if (fits) return scroll;
  const centred = length + 2 * margin > size ? start - margin : start + length / 2 - size / 2;
  return Math.max(0, centred);
}
