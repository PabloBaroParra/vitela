// What the "Move to" and "Resize" dialogs' typed numbers mean: the box's
// top-left corner, and its width and height with that corner kept — in
// points on the page as drawn (from its top-left corner, the page's turn
// applied), the frame every other annotation edit of this shell uses.
// Checked here so a refusal is said in the dialog, before the core is asked:
// the box must stay on the page and keep the size floor of its kind. A
// length left as it was shown is the length itself, not its rounding, and is
// never refused: only what the user typed is checked. Pure.

import type { PageRect, PageSize } from '@vitela/core';
import type { Size } from './drawing';

export type MoveAnswer = { readonly left: number; readonly top: number } | { readonly error: string };
export type ResizeAnswer = Size | { readonly error: string };

/** A length as the dialogs show it: two decimals at most. */
export function formatPt(value: number): string {
  return String(Math.round(value * 100) / 100);
}

/** `text` as a number, a decimal comma accepted; `null` if it is not one. */
function parsePt(text: string): number | null {
  const trimmed = text.trim().replace(',', '.');
  if (trimmed === '') return null;
  const value = Number(trimmed);
  return Number.isFinite(value) ? value : null;
}

/** The length typed as `text` in the field `name` (which showed `now`), or why it cannot be used. */
function within(name: string, text: string, now: number, min: number, max: number): number | string {
  const value = parsePt(text);
  if (value === null) return `${name} must be a number of points.`;
  if (value === parsePt(formatPt(now))) return now;
  if (value < min || value > max) return `${name} must be between ${formatPt(min)} and ${formatPt(max)} points.`;
  return value;
}

/** Where the typed `left` and `top` put the top-left corner of `bounds`. */
export function parseMove(left: string, top: string, bounds: PageRect, page: PageSize): MoveAnswer {
  const x = within('Left', left, bounds.left, 0, Math.max(0, page.widthPt - bounds.width));
  if (typeof x === 'string') return { error: x };
  const y = within('Top', top, bounds.top, 0, Math.max(0, page.heightPt - bounds.height));
  if (typeof y === 'string') return { error: y };
  return { left: x, top: y };
}

/** The typed size of `rect`, whose top-left corner is kept; never under `floor`. */
export function parseResize(width: string, height: string, rect: PageRect, floor: Size, page: PageSize): ResizeAnswer {
  const w = within('Width', width, rect.width, floor.width, Math.max(floor.width, page.widthPt - rect.left));
  if (typeof w === 'string') return { error: w };
  const h = within('Height', height, rect.height, floor.height, Math.max(floor.height, page.heightPt - rect.top));
  if (typeof h === 'string') return { error: h };
  return { width: Math.max(w, floor.width), height: Math.max(h, floor.height) };
}

/** The other side of `rect`'s proportions for a typed `side`; `null` while it is not a size. */
export function proportional(text: string, side: 'width' | 'height', rect: PageRect): string | null {
  const value = parsePt(text);
  if (value === null || value <= 0) return null;
  return formatPt(side === 'width' ? (value * rect.height) / rect.width : (value * rect.width) / rect.height);
}
