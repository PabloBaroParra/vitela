// The vector-path edits of the "Edit content" mode (B26 T-263), pure: which
// handles a selected path gets, the box a handle drags it into, and which
// path is the edited one once the page is read again.
//
// What an edit writes, whether it is allowed, and how a second move or
// resize of the same path folds into its step are the core's. A resize keeps
// the line width (the core rewrites the coordinates), and a side the path is
// flat along — a rule's height — stays flat: there is nothing to stretch,
// as the core and the GTK shell (`dragged_rect`) decide it. So a rule gets
// only the two handles at its ends.
//
// Unlike an image's, a corner resizes freely, as the native shells do: a
// table frame or a shaded box has no proportions to keep. Shift keeps them.

import type { ContentPath, PageContent, PageRect } from '@vitela/core';
import { nearestTo } from './contentModel';
import { IMAGE_HANDLES, type ImageHandle } from './images';

/** Below this, in points, a path's side counts as flat: the core's own threshold (`model/path.rs`). */
const FLAT_PT = 1e-9;

/**
 * The smallest side a drag leaves on the axis it moves, in points. A side
 * already thinner (a 0.5 pt table border) keeps its size as its floor, so a
 * resize never inflates it; the image minimum does not apply to paths.
 */
export const MIN_PATH_PT = 1;

const flat = (size: number) => Math.abs(size) < FLAT_PT;

/** The handles a path at `rect` is resized from: all eight, or only those along the side it has. */
export function pathHandles(rect: PageRect): readonly ImageHandle[] {
  const across = flat(rect.width);
  const down = flat(rect.height);
  if (across && down) return [];
  if (down) return ['e', 'w'];
  if (across) return ['n', 's'];
  return IMAGE_HANDLES;
}

/**
 * The box `rect` becomes when `handle` is dragged by `dx, dy` points on the
 * page as drawn. The opposite corner (or side) stays; a corner is free
 * unless `proportional` (Shift); a side the handle does not move, or the
 * path is flat along, stays as it is and in place.
 */
export function resizedPathRect(rect: PageRect, handle: ImageHandle, dx: number, dy: number, proportional: boolean): PageRect {
  // Which sides move: -1 the left (top), 1 the right (bottom), 0 neither.
  const sx = flat(rect.width) ? 0 : handle.includes('w') ? -1 : handle.includes('e') ? 1 : 0;
  const sy = flat(rect.height) ? 0 : handle.startsWith('n') ? -1 : handle.startsWith('s') ? 1 : 0;
  const minWidth = Math.min(MIN_PATH_PT, rect.width);
  const minHeight = Math.min(MIN_PATH_PT, rect.height);
  let width = rect.width + sx * dx;
  let height = rect.height + sy * dy;
  if (sx !== 0 && sy !== 0 && proportional) {
    // The pointer's larger movement sets the scale, unless its axis is too
    // thin to measure one by: a 0.5 pt bar scales by its length.
    const byWidth = Math.abs(dx) >= Math.abs(dy) ? rect.width >= MIN_PATH_PT : rect.height < MIN_PATH_PT;
    const scale = byWidth ? width / rect.width : height / rect.height;
    const kept = Math.max(scale, minWidth / rect.width, minHeight / rect.height);
    width = rect.width * kept;
    height = rect.height * kept;
  } else {
    width = Math.max(width, minWidth);
    height = Math.max(height, minHeight);
  }
  return {
    left: sx === -1 ? rect.left + rect.width - width : rect.left,
    top: sy === -1 ? rect.top + rect.height - height : rect.top,
    width,
    height,
  };
}

/**
 * The edited path in a page read after the edit: the one closest to
 * `expected` (where it should be now) among those overlapping it; `null`
 * when it was removed (`expected` is `null`) or nothing overlaps.
 */
export function followPath(content: PageContent, expected: PageRect | null): ContentPath | null {
  return nearestTo(content.paths, expected);
}
