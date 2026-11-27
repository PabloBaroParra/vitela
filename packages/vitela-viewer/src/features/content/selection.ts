// Several content items at once and the content clipboard (B26 T-262), pure:
// what a marquee or a SHIFT/CTRL+click gathers, the ids the core takes, how
// moved items are found again once their page is read anew, where a paste
// lands and what it added, and which key presses are the clipboard's.
//
// Every rule about the content itself is the core's (`moveContentSelection`,
// `removeContentSelection`, `copyContentSelection`, `pasteContent`): which
// moves fold, what a pending paste lets a selection do, what a copy refuses.
// This only decides what to ask for, as the native shells do.

import type { ContentClip, ContentItem, ContentMembers, PageContent, PagePoint, PageRect } from '@vitela/core';
import { isTextField } from '../document/historyKeys';
import { sameItem } from './contentModel';
import { followImage } from './images';
import { followRun } from './textRuns';

/** The ids of `items` sorted by kind, as the core's selection edits take them. */
export function membersOf(items: readonly ContentItem[]): ContentMembers {
  const ids = (kind: ContentItem['kind']) => items.filter((item) => item.kind === kind).map((item) => item.id);
  return { runIds: ids('run'), imageIds: ids('image'), pathIds: ids('path') };
}

/** `items` with `item` added, or taken out when it is there. */
export function toggled(items: readonly ContentItem[], item: ContentItem): ContentItem[] {
  return items.some((member) => sameItem(member, item)) ? items.filter((member) => !sameItem(member, item)) : [...items, item];
}

/** The box two corners of a drag span, whichever way it went. */
export function marqueeRect(from: PagePoint, to: PagePoint): PageRect {
  return {
    left: Math.min(from.x, to.x),
    top: Math.min(from.y, to.y),
    width: Math.abs(to.x - from.x),
    height: Math.abs(to.y - from.y),
  };
}

/** Every item of `content` whose box touches `area` (edges count: a hairline rule has no height), runs first. */
export function gathered(content: PageContent, area: PageRect): ContentItem[] {
  return [...content.runs, ...content.images, ...content.paths].filter((item) => touches(item.rect, area));
}

/** The union of the items' boxes; `null` for none. */
export function boundsOf(items: readonly ContentItem[]): PageRect | null {
  if (items.length === 0) return null;
  const left = Math.min(...items.map((item) => item.rect.left));
  const top = Math.min(...items.map((item) => item.rect.top));
  const right = Math.max(...items.map((item) => item.rect.left + item.rect.width));
  const bottom = Math.max(...items.map((item) => item.rect.top + item.rect.height));
  return { left, top, width: right - left, height: bottom - top };
}

/** One moved item to find again: its kind, where it should be now, and a run's text. */
export type Sought =
  | { readonly kind: 'run'; readonly rect: PageRect; readonly text: string }
  | { readonly kind: 'image' | 'path'; readonly rect: PageRect };

/** What to find again after `items` move by `dx, dy` (drawn points). */
export function soughtOf(items: readonly ContentItem[], dx: number, dy: number): Sought[] {
  return items.map((item) => {
    const rect = { ...item.rect, left: item.rect.left + dx, top: item.rect.top + dy };
    return item.kind === 'run' ? { kind: 'run', rect, text: item.text } : { kind: item.kind, rect };
  });
}

/**
 * The moved items in a page read after the move: each found by its kind and
 * place (a run by its text first), the closest of those overlapping where it
 * should be, none taken twice. Ids are valid only against the read they came
 * from, so items are found by place, not by id; what is not found is left out.
 */
export function followGroup(content: PageContent, sought: readonly Sought[]): ContentItem[] {
  const found: ContentItem[] = [];
  const unused = <T extends ContentItem>(items: readonly T[]): T[] => items.filter((item) => !found.some((taken) => sameItem(taken, item)));
  for (const target of sought) {
    const item =
      target.kind === 'run'
        ? followRun({ ...content, runs: unused(content.runs) }, target.rect, target.text)
        : target.kind === 'image'
          ? followImage({ ...content, images: unused(content.images) }, target.rect)
          : closestPath(unused(content.paths), target.rect);
    if (item) found.push(item);
  }
  return found;
}

/**
 * Where a paste at `at` is drawn: the clip's box hangs from its top-left
 * corner in the page's own frame, so on a page drawn turned it turns about
 * that corner with the page (as an inserted line does).
 */
export function pastedRect(at: PagePoint, clip: ContentClip, rotationDegrees: number): PageRect {
  const [, , width, height] = clip.bbox;
  switch (rotationDegrees) {
    case 90:
      return { left: at.x - height, top: at.y, width: height, height: width };
    case 180:
      return { left: at.x - width, top: at.y - height, width, height };
    case 270:
      return { left: at.x, top: at.y - width, width: height, height: width };
    default:
      return { left: at.x, top: at.y, width, height };
  }
}

/**
 * What a paste added where it landed (`landed`): the pending items of
 * `content` that were not pending in `before`, the page read when it was
 * asked for. The core moves or removes a paste only as exactly its own items,
 * so an earlier paste under it or an unsaved insert it covers stays out. Ids
 * are valid only per read: an item is matched by kind and place (a run by its
 * text too), one for one in reading order, so of two stacked pastes the later
 * one's items are left over.
 */
export function pastedItems(content: PageContent, landed: PageRect, before: PageContent | null = null): ContentItem[] {
  const earlier = before ? [...before.runs, ...before.images, ...before.paths].filter((item) => item.pending) : [];
  return gathered(content, landed).filter((item) => {
    if (!item.pending) return false;
    const at = earlier.findIndex((old) => alike(old, item));
    if (at < 0) return true;
    earlier.splice(at, 1);
    return false;
  });
}

function alike(a: ContentItem, b: ContentItem): boolean {
  const near = (x: number, y: number) => Math.abs(x - y) < 0.01;
  const same = near(a.rect.left, b.rect.left) && near(a.rect.top, b.rect.top) && near(a.rect.width, b.rect.width) && near(a.rect.height, b.rect.height);
  return a.kind === b.kind && same && (a.kind !== 'run' || (b.kind === 'run' && a.text === b.text));
}

/** The pointer over a page, in drawn points. */
export interface PagePointer {
  readonly pageIndex: number;
  readonly point: PagePoint;
}

/**
 * Where a paste goes, as the Windows shell decides it: at the pointer while
 * it is over a page; else on the first page in view, at the place the clip
 * was copied from — kept on that page when it is smaller. `null` when that
 * page does not exist.
 */
export function pastePoint(
  clip: ContentClip,
  pointer: PagePointer | null,
  firstVisible: number,
  pages: readonly { readonly widthPt: number; readonly heightPt: number }[],
): { readonly pageIndex: number; readonly at: PagePoint } | null {
  if (pointer && pages[pointer.pageIndex]) return { pageIndex: pointer.pageIndex, at: pointer.point };
  const page = pages[firstVisible];
  if (!page) return null;
  const { left, top, width, height } = clip.rect;
  const clamp = (value: number, room: number) => Math.min(Math.max(value, 0), Math.max(room, 0));
  return { pageIndex: firstVisible, at: { x: clamp(left, page.widthPt - width), y: clamp(top, page.heightPt - height) } };
}

/** What the status line says while several items are selected. */
export function describeGroup(count: number, pageIndex: number): string {
  const what = count === 1 ? '1 item' : `${count} items`;
  const them = count === 1 ? 'it' : 'them';
  return `${what} selected on page ${pageIndex + 1}. Drag ${them}, press Delete, or copy ${them}.`;
}

/** A clipboard key the content mode answers. */
export type ClipboardAction = 'copy' | 'cut' | 'paste';

/**
 * Whether `event` is a plain Ctrl/⌘ + C, X or V the content mode should
 * answer: not one typed into a field, and — for a copy or a cut — not one
 * meant for text the browser itself has selected on the page.
 */
export function clipboardAction(event: KeyboardEvent, domSelection: Selection | null): ClipboardAction | null {
  if (event.defaultPrevented || !(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return null;
  const action = ({ c: 'copy', x: 'cut', v: 'paste' } as const)[event.key.toLowerCase() as 'c' | 'x' | 'v'];
  if (!action || isTextField(event.target)) return null;
  if (action !== 'paste' && domSelection && !domSelection.isCollapsed) return null;
  return action;
}

function touches(a: PageRect, b: PageRect): boolean {
  return a.left <= b.left + b.width && b.left <= a.left + a.width && a.top <= b.top + b.height && b.top <= a.top + a.height;
}

function closestPath<T extends ContentItem>(paths: readonly T[], expected: PageRect): T | null {
  let best: T | null = null;
  let bestDistance = Infinity;
  for (const path of paths) {
    if (!touches(path.rect, expected)) continue;
    const distance =
      Math.abs(path.rect.left - expected.left) +
      Math.abs(path.rect.top - expected.top) +
      Math.abs(path.rect.width - expected.width) +
      Math.abs(path.rect.height - expected.height);
    if (distance < bestDistance) {
      best = path;
      bestDistance = distance;
    }
  }
  return best;
}
