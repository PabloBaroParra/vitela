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

/** Room kept around a target, per axis. */
export type Margins = Point;

/** `margins`, less what a target that fits leaves no room for: it is shown whole rather than clipped by its own margin. */
export function fittingMargins(
  viewport: Pick<Viewport, 'width' | 'height'>,
  target: Pick<Rect, 'width' | 'height'>,
  margins: Margins,
): Margins {
  const fit = (size: number, length: number, margin: number) => Math.min(margin, Math.max(0, (size - length) / 2));
  return { x: fit(viewport.width, target.width, margins.x), y: fit(viewport.height, target.height, margins.y) };
}

/**
 * The scroll (`x` = scrollLeft, `y` = scrollTop) that shows `target` with
 * `margin` px around it, or `null` if it already shows. Each axis moves only
 * if the target is not inside it, and then centres the target on that axis
 * (a target bigger than the viewport starts at its near edge). Never
 * negative; the browser clamps the far end.
 */
export function revealScroll(viewport: Viewport, target: Rect, margin: number | Margins): Point | null {
  const margins = typeof margin === 'number' ? { x: margin, y: margin } : margin;
  const x = axis(viewport.left, viewport.width, target.left, target.width, margins.x);
  const y = axis(viewport.top, viewport.height, target.top, target.height, margins.y);
  if (x === viewport.left && y === viewport.top) return null;
  return { x, y };
}

function axis(scroll: number, size: number, start: number, length: number, margin: number): number {
  const fits = start - margin >= scroll && start + length + margin <= scroll + size;
  if (fits) return scroll;
  const centred = length + 2 * margin > size ? start - margin : start + length / 2 - size / 2;
  return Math.max(0, centred);
}
