// The image edits of the "Edit content" mode (B26 T-260), pure: the box a
// resize handle drags an image into, where the eight handles sit, and which
// image is the edited one once the page is read again.
//
// What an edit writes, whether it is allowed, how a second move or resize
// of the same image folds into its step, the size an inserted image gets and
// whether a picture can be replaced are all the core's. This only decides
// what to ask it for.
//
// Handles: four corners and four side middles. A corner keeps the image's
// proportions unless Shift is held; a side changes one dimension. The native
// shells (GTK, WinUI, Compose) resize from four corners, freely; the web
// keeps a photo from being squashed by default, as most editors do, and
// Shift gives the native behaviour.

import type { ContentImage, PageContent, PageRect } from '@vitela/core';

/** A resize handle: a compass point of the image's box. */
export type ImageHandle = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w';

export const IMAGE_HANDLES: readonly ImageHandle[] = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];

/** The smallest side a resize leaves, in points: an image of no size could never be grabbed again. */
export const MIN_IMAGE_PT = 4;

/** A handle's side, in CSS px: the same under the pointer at every zoom. */
export const HANDLE_PX = 10;

/** Which sides a handle moves: -1 the left (top), 1 the right (bottom), 0 neither. */
function sidesOf(handle: ImageHandle): { readonly x: -1 | 0 | 1; readonly y: -1 | 0 | 1 } {
  return {
    x: handle.includes('w') ? -1 : handle.includes('e') ? 1 : 0,
    y: handle.startsWith('n') ? -1 : handle.startsWith('s') ? 1 : 0,
  };
}

/**
 * The box `rect` becomes when `handle` is dragged by `dx, dy` points on the
 * page as drawn. The opposite corner (or side) stays; a corner keeps the
 * proportions unless `free` (Shift); no side goes below `MIN_IMAGE_PT`.
 */
export function resizedRect(rect: PageRect, handle: ImageHandle, dx: number, dy: number, free: boolean): PageRect {
  const sides = sidesOf(handle);
  let width = sides.x === 0 ? rect.width : rect.width + sides.x * dx;
  let height = sides.y === 0 ? rect.height : rect.height + sides.y * dy;
  if (sides.x !== 0 && sides.y !== 0 && !free && rect.width > 0 && rect.height > 0) {
    // The side pulled further (relative to its length, either way) sets the scale.
    const across = width / rect.width;
    const down = height / rect.height;
    const scale = Math.abs(across - 1) >= Math.abs(down - 1) ? across : down;
    const floor = Math.max(MIN_IMAGE_PT / rect.width, MIN_IMAGE_PT / rect.height);
    const kept = Math.max(scale, floor);
    width = rect.width * kept;
    height = rect.height * kept;
  } else {
    width = Math.max(width, MIN_IMAGE_PT);
    height = Math.max(height, MIN_IMAGE_PT);
  }
  const right = rect.left + rect.width;
  const bottom = rect.top + rect.height;
  return {
    left: sides.x === -1 ? right - width : rect.left,
    top: sides.y === -1 ? bottom - height : rect.top,
    width,
    height,
  };
}

/** Where `handle` is drawn for an image at `rect`, in CSS px at `scale` (px per point). */
export function handleBox(rect: PageRect, handle: ImageHandle, scale: number): { left: number; top: number; width: number; height: number } {
  const sides = sidesOf(handle);
  const x = (rect.left + (rect.width * (sides.x + 1)) / 2) * scale;
  const y = (rect.top + (rect.height * (sides.y + 1)) / 2) * scale;
  return { left: x - HANDLE_PX / 2, top: y - HANDLE_PX / 2, width: HANDLE_PX, height: HANDLE_PX };
}

/**
 * The edited image in a page read after the edit: the one whose box is
 * closest to `expected` (where it should be now) among those overlapping it.
 * `null` when it was removed (`expected` is `null`) or nothing overlaps. Ids
 * are valid only against the read they came from, so the image is found by
 * place, not by id.
 */
export function followImage(content: PageContent, expected: PageRect | null): ContentImage | null {
  if (!expected) return null;
  let best: ContentImage | null = null;
  let bestDistance = Infinity;
  for (const image of content.images) {
    if (!overlaps(image.rect, expected)) continue;
    const distance =
      Math.abs(image.rect.left - expected.left) +
      Math.abs(image.rect.top - expected.top) +
      Math.abs(image.rect.width - expected.width) +
      Math.abs(image.rect.height - expected.height);
    if (distance < bestDistance) {
      best = image;
      bestDistance = distance;
    }
  }
  return best;
}

function overlaps(a: PageRect, b: PageRect): boolean {
  return a.left <= b.left + b.width && b.left <= a.left + a.width && a.top <= b.top + b.height && b.top <= a.top + a.height;
}
