// The "Edit content" mode's decisions (B26 T-258b), pure: which pages to
// parse, what a parse is cached under, which item is under the pointer, and
// what the status line says about it.
//
// Parsing is the core's (`Engine.readPageContent`) and costs a walk of the
// page's whole content stream, so the mode parses only the pages in view and
// one on each side, never the document: a 1619-page file froze the Linux
// shell when every page was parsed at once. A parse is valid for exactly one
// page (its id) turned one way (its rotation: an undo of a turn keeps the id
// and the revision), one set of applied content edits (the paint revision)
// and one in-memory base (the core's base generation), so those four are its
// key. Reads go out a few at a time (`readsToSend`), so a fast scroll leaves
// no backlog of pages it has already passed.

import type { ContentItem, PageContent, PagePoint, PageRect } from '@vitela/core';

/** How many pages on each side of the visible ones are parsed too. */
export const CONTENT_MARGIN_PAGES = 1;

/** How far, in points, the pointer may miss a thin item (a hairline rule) and still find it. */
export const HIT_SLOP_PT = 3;

/** How many page reads are asked of the core at once. */
export const CONTENT_READS_IN_FLIGHT = 2;

/** The parse of one page is valid for this page id, turn, paint revision and base generation only. */
export function contentKey(pageId: number, rotationDegrees: number, paintRevision: number, baseGeneration: number): string {
  return `${pageId}/${rotationDegrees}/${paintRevision}/${baseGeneration}`;
}

/**
 * The positions to parse: the visible ones and `margin` on each side, kept
 * inside the document. Empty when nothing is visible.
 */
export function pagesToParse(
  visible: { readonly first: number; readonly last: number } | null,
  pageCount: number,
  margin = CONTENT_MARGIN_PAGES,
): number[] {
  if (!visible || pageCount <= 0) return [];
  const first = Math.max(0, visible.first - margin);
  const last = Math.min(pageCount - 1, visible.last + margin);
  const pages: number[] = [];
  for (let index = first; index <= last; index++) pages.push(index);
  return pages;
}

/** One page's parse as the mode holds it. */
export type PageContentState =
  | { readonly status: 'loading' }
  | { readonly status: 'ready'; readonly content: PageContent }
  | { readonly status: 'failed'; readonly message: string };

/**
 * The parses the mode holds, by `contentKey`. Only the keys still wanted are
 * kept: a page scrolled out of reach, or one an edit repainted, is dropped,
 * so memory follows the pages in view and not the document.
 */
export function keepWanted(
  held: ReadonlyMap<string, PageContentState>,
  wanted: ReadonlySet<string>,
): Map<string, PageContentState> {
  const kept = new Map<string, PageContentState>();
  for (const [key, state] of held) if (wanted.has(key)) kept.set(key, state);
  return kept;
}

/** One page to read: its position now, and the key its parse is held under. */
export interface WantedPage {
  readonly index: number;
  readonly key: string;
}

/**
 * Which of the `wanted` pages to ask the core for now: those still waiting
 * (`loading`) whose read is not already `inFlight`, in order, so that no more
 * than `limit` are in flight at once. What is not sent now is decided again
 * when a read lands, against the pages wanted then: a page a fast scroll has
 * passed is never read.
 */
export function readsToSend(
  wanted: readonly WantedPage[],
  held: ReadonlyMap<string, PageContentState>,
  inFlight: ReadonlySet<string>,
  limit = CONTENT_READS_IN_FLIGHT,
): WantedPage[] {
  const room = limit - inFlight.size;
  if (room <= 0) return [];
  return wanted.filter(({ key }) => held.get(key)?.status === 'loading' && !inFlight.has(key)).slice(0, room);
}

/** Every item of a page, in one list: runs, images, paths. */
export function contentItems(content: PageContent): ContentItem[] {
  return [...content.runs, ...content.images, ...content.paths];
}

/**
 * The item under `point` (drawn points): the smallest one `hits`, so a run
 * painted over an image is found rather than the image, and a rule with a
 * box of no height is still found. `null` over nothing.
 */
export function itemAt(content: PageContent, point: PagePoint, slop = HIT_SLOP_PT): ContentItem | null {
  let found: ContentItem | null = null;
  let foundArea = Infinity;
  for (const item of contentItems(content)) {
    if (!hits(item, point, slop)) continue;
    const area = (item.rect.width + 2 * slop) * (item.rect.height + 2 * slop);
    if (area < foundArea) {
      found = item;
      foundArea = area;
    }
  }
  return found;
}

/**
 * Whether a press at `point` means `item`: anywhere in its box widened by
 * `slop`, except for an outline (a stroked path), which is hit only within
 * `slop` of its edges — further inside a frame is its cell, not its border,
 * as the native shells decide it (GTK's `path_hit`).
 */
export function hits(item: ContentItem, point: PagePoint, slop = HIT_SLOP_PT): boolean {
  const { rect } = item;
  if (!holds(rect, point, slop)) return false;
  if (item.kind !== 'path' || item.paint !== 'stroke') return true;
  const inner = { left: rect.left + slop, top: rect.top + slop, width: rect.width - 2 * slop, height: rect.height - 2 * slop };
  return inner.width <= 0 || inner.height <= 0 || !holds(inner, point, 0);
}

/**
 * The item among `items` whose box is closest to `expected` (where an edited
 * item should be now) among those overlapping it; `null` when `expected` is
 * `null` (it was removed) or nothing overlaps. Ids are valid only against
 * the read they came from, so an edited item is found again by place.
 */
export function nearestTo<T extends { readonly rect: PageRect }>(items: readonly T[], expected: PageRect | null): T | null {
  if (!expected) return null;
  let best: T | null = null;
  let bestDistance = Infinity;
  for (const item of items) {
    if (!overlaps(item.rect, expected)) continue;
    const distance =
      Math.abs(item.rect.left - expected.left) +
      Math.abs(item.rect.top - expected.top) +
      Math.abs(item.rect.width - expected.width) +
      Math.abs(item.rect.height - expected.height);
    if (distance < bestDistance) {
      best = item;
      bestDistance = distance;
    }
  }
  return best;
}

function overlaps(a: PageRect, b: PageRect): boolean {
  return a.left <= b.left + b.width && b.left <= a.left + a.width && a.top <= b.top + b.height && b.top <= a.top + a.height;
}

function holds(rect: PageRect, point: PagePoint, slop: number): boolean {
  return (
    point.x >= rect.left - slop &&
    point.x <= rect.left + rect.width + slop &&
    point.y >= rect.top - slop &&
    point.y <= rect.top + rect.height + slop
  );
}

/** Whether two boxes are exactly the same box. */
export function sameRect(a: PageRect, b: PageRect): boolean {
  return a.left === b.left && a.top === b.top && a.width === b.width && a.height === b.height;
}

/** Whether two items are the same item of the same page. */
export function sameItem(a: ContentItem | null, b: ContentItem | null): boolean {
  return !!a && !!b && a.kind === b.kind && a.id === b.id && a.pageIndex === b.pageIndex;
}

/** What the status line says while the mode is on and nothing is selected. */
export const CONTENT_HINT = 'Point at text, an image or a shape on a page, and click to select it.';

/** What the status line says about a selected item. */
export function describeItem(item: ContentItem): string {
  const page = `on page ${item.pageIndex + 1}`;
  switch (item.kind) {
    case 'run':
      return item.editable
        ? `Text “${item.text.trim()}” selected ${page}.`
        : `Text “${item.text.trim()}” selected ${page}. Its font does not say what all of its characters are, so it cannot be retyped.`;
    case 'image':
      return `Image selected ${page}.`;
    case 'path':
      return `Shape selected ${page}.`;
  }
}
