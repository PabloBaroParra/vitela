// The drawing tools' rules: which tools there are, the colours they offer,
// and what a pointer gesture on a page becomes — a stroke, a traced box or a
// default-sized box at a click — and then which annotation edit. Pure.
//
// Everything is in points on the page as drawn (origin top-left, the page's
// turn applied), the engine's own geometry: the pointer only needs dividing
// by the zoom (`toPagePoint`), and the core turns the result into PDF space
// whatever the page's `/Rotate`. Sizes and thresholds are the native shells'.

import type { AnnotationEdit, PagePoint, PageRect, PageSize, Rgb } from '@vitela/core';
import { isSideways } from './geometry';

/**
 * What a press on the page does: select text, draw one kind of annotation,
 * or stamp the picked image (`stamp`, armed by picking one: see `useStamps`).
 */
export type DrawTool = 'select' | 'ink' | 'shape' | 'note' | 'freeText' | 'stamp';

/** The tools that take the pointer away from text selection. */
export type ArmedTool = Exclude<DrawTool, 'select'>;

/** The tools armed by their own button; Image (`stamp`) is armed by picking a file. */
export const DRAW_TOOLS: readonly { readonly tool: DrawTool; readonly label: string }[] = [
  { tool: 'select', label: 'Select' },
  { tool: 'ink', label: 'Ink' },
  { tool: 'shape', label: 'Shape' },
  { tool: 'note', label: 'Note' },
  { tool: 'freeText', label: 'Free text' },
];

/** The colours ink and shapes can take; the first is the native shells' `DEFAULT_COLOR`. */
export const PALETTE: readonly { readonly name: string; readonly color: Rgb }[] = [
  { name: 'Yellow', color: { r: 255, g: 220, b: 0 } },
  { name: 'Red', color: { r: 220, g: 40, b: 40 } },
  { name: 'Blue', color: { r: 30, g: 100, b: 230 } },
  { name: 'Green', color: { r: 20, g: 150, b: 60 } },
  { name: 'Black', color: { r: 0, g: 0, b: 0 } },
];

/** Whether `tool` draws with the chosen colour (notes and free text have none in the core). */
export function usesColor(tool: DrawTool): boolean {
  return tool === 'ink' || tool === 'shape';
}

/** How far the pointer must travel from the press, in points, for a drag rather than a click. */
export const MIN_DRAG_PT = 8;
/** The smallest side of a traced shape, so a flat sweep still leaves something visible. */
export const MIN_SHAPE_PT = 4;
/** A shape placed with a click (Linux's `CLICK_SIZE_PT`). */
export const SHAPE_CLICK_SIZE: Size = { width: 144, height: 36 };
/** A note's icon box; a note is placed with a click. */
export const NOTE_SIZE: Size = { width: 24, height: 24 };
/**
 * A text box placed with a click: a sentence at 12 pt, two lines (Linux and
 * Android). Like `FREE_TEXT_MIN_SIZE`, it is the size of the box's own text
 * frame: on a quarter-turned page the drawn box has it sideways (`drawnSize`).
 */
export const FREE_TEXT_CLICK_SIZE: Size = { width: 200, height: 50 };
/**
 * The smallest text box the core accepts for its default 12 pt Helvetica:
 * one glyph wide and one line high, padding included (`min_free_text_size`).
 */
export const FREE_TEXT_MIN_SIZE: Size = { width: 16, height: 17.8 };
/** Ink points closer than this to the last one kept add nothing to the stroke. */
const INK_STEP_PT = 0.5;

export interface Size {
  readonly width: number;
  readonly height: number;
}

/** What a finished gesture on one page asks for, in drawn points. */
export type Gesture =
  | { readonly kind: 'stroke'; readonly points: readonly PagePoint[] }
  | { readonly kind: 'box'; readonly rect: PageRect }
  /** Where a stamp goes: the anchor the core places the image's top-left corner at. */
  | { readonly kind: 'point'; readonly point: PagePoint };

/** `point` moved inside the page. */
export function clampPoint(point: PagePoint, page: PageSize): PagePoint {
  return { x: clamp(point.x, 0, page.widthPt), y: clamp(point.y, 0, page.heightPt) };
}

/** Whether the pointer went far enough from `start` to `end` for a drag (distance, not per axis). */
export function isDrag(start: PagePoint, end: PagePoint): boolean {
  return Math.hypot(end.x - start.x, end.y - start.y) >= MIN_DRAG_PT;
}

/**
 * The box traced from `start` to `end`, whichever way the drag went, each
 * side at least `floor`, and wholly on the page. A floor grows the box away
 * from the press, never past the page's edge.
 */
export function tracedRect(start: PagePoint, end: PagePoint, floor: Size, page: PageSize): PageRect {
  const a = clampPoint(start, page);
  const b = clampPoint(end, page);
  const width = Math.max(Math.abs(b.x - a.x), floor.width);
  const height = Math.max(Math.abs(b.y - a.y), floor.height);
  const left = b.x < a.x ? a.x - width : a.x;
  const top = b.y < a.y ? a.y - height : a.y;
  return fitOnPage({ left, top, width, height }, page);
}

/** A `size` box with its top-left corner at `point`, moved (never shrunk, unless larger) to sit on the page. */
export function clickRect(point: PagePoint, size: Size, page: PageSize): PageRect {
  return fitOnPage({ left: point.x, top: point.y, width: size.width, height: size.height }, page);
}

/** `rect` slid inside the page; a side longer than the page is cut to it. */
export function fitOnPage(rect: PageRect, page: PageSize): PageRect {
  const width = Math.min(rect.width, page.widthPt);
  const height = Math.min(rect.height, page.heightPt);
  return {
    left: clamp(rect.left, 0, page.widthPt - width),
    top: clamp(rect.top, 0, page.heightPt - height),
    width,
    height,
  };
}

/** `point` appended to a stroke on the page, unless it is too close to the last one to matter. */
export function extendStroke(points: readonly PagePoint[], point: PagePoint, page: PageSize): PagePoint[] {
  const next = clampPoint(point, page);
  const last = points[points.length - 1];
  if (last && Math.hypot(next.x - last.x, next.y - last.y) < INK_STEP_PT) return [...points];
  return [...points, next];
}

/**
 * What `tool` makes of a press at `points[0]` dragged through the rest:
 * ink keeps the stroke (a stroke needs two points), a shape or a text box
 * the traced box (or the default one for a click), a note the default box
 * at the click, a stamp the press itself (the core sizes it). A text box's
 * minimum and default sizes are its text frame's, so they turn with the
 * page. `null` when the gesture makes nothing.
 */
export function finishGesture(tool: ArmedTool, points: readonly PagePoint[], page: PageSize): Gesture | null {
  const start = points[0];
  const end = points[points.length - 1];
  if (!start || !end) return null;
  switch (tool) {
    case 'ink':
      return points.length >= 2 ? { kind: 'stroke', points } : null;
    case 'note':
      return { kind: 'box', rect: clickRect(clampPoint(start, page), NOTE_SIZE, page) };
    case 'stamp':
      return { kind: 'point', point: clampPoint(start, page) };
    case 'shape':
      return {
        kind: 'box',
        rect: isDrag(start, end)
          ? tracedRect(start, end, { width: MIN_SHAPE_PT, height: MIN_SHAPE_PT }, page)
          : clickRect(clampPoint(start, page), SHAPE_CLICK_SIZE, page),
      };
    case 'freeText':
      return {
        kind: 'box',
        rect: isDrag(start, end)
          ? tracedRect(start, end, drawnSize(FREE_TEXT_MIN_SIZE, page.rotationDegrees), page)
          : clickRect(clampPoint(start, page), drawnSize(FREE_TEXT_CLICK_SIZE, page.rotationDegrees), page),
      };
  }
}

/** The edit a stroke or a shape makes on `page`, in `color`. Notes and free text wait for their text. */
export function drawnEdit(tool: 'ink' | 'shape', page: number, gesture: Gesture, color: Rgb): AnnotationEdit | null {
  if (tool === 'ink') return gesture.kind === 'stroke' ? { kind: 'addInk', page, points: gesture.points, color } : null;
  return gesture.kind === 'box' ? { kind: 'addShape', page, rect: gesture.rect, color } : null;
}

/** The edit a note or text box makes once its text is typed. */
export function textEdit(tool: 'note' | 'freeText', page: number, rect: PageRect, contents: string): AnnotationEdit {
  return tool === 'note'
    ? { kind: 'addNote', page, rect, contents }
    : { kind: 'addFreeText', page, rect, contents };
}

/**
 * A text frame `size` as a box drawn on a page turned `rotationDegrees`:
 * the same upright or upside down, its sides swapped on a quarter turn (the
 * inverse of the swap in `freeTextFrame`).
 */
export function drawnSize(size: Size, rotationDegrees: number): Size {
  return isSideways(rotationDegrees) ? { width: size.height, height: size.width } : size;
}

/** Whether `text` is nothing but white space: a note or text box needs something in it. */
export function isBlank(text: string): boolean {
  return text.trim().length === 0;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), Math.max(min, max));
}
