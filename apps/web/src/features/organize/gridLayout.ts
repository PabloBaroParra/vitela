// Where every card of the Organize grid sits, in CSS pixels: equal cells in
// rows as wide as the viewport, so the grid is virtualized by row and a drop
// position is found by arithmetic, without measuring the DOM. Pure.

import type { Point, Rect } from '../viewer/layout';

/** One card: the thumbnail box plus its page number underneath. */
export const CARD = { width: 168, height: 232 } as const;
export const GRID_GAP = 16;
export const GRID_PADDING = 16;

export interface GridLayout {
  readonly pageCount: number;
  readonly columns: number;
  readonly rows: number;
  /** Size of the scrollable content. */
  readonly width: number;
  readonly height: number;
}

/** Where a dragged run would land: in front of page `index` (`pageCount` = after the last). */
export interface DropSlot {
  readonly index: number;
  /** The row and the gap within it (`0..columns`) to draw the drop marker in. */
  readonly row: number;
  readonly column: number;
}

const STEP_X = CARD.width + GRID_GAP;
const STEP_Y = CARD.height + GRID_GAP;

export function layoutGrid(pageCount: number, viewportWidth: number): GridLayout {
  const columns = Math.max(1, Math.floor((viewportWidth - 2 * GRID_PADDING + GRID_GAP) / STEP_X));
  const rows = Math.ceil(pageCount / columns);
  return {
    pageCount,
    columns,
    rows,
    width: Math.max(viewportWidth, 2 * GRID_PADDING + CARD.width),
    height: rows === 0 ? 2 * GRID_PADDING : 2 * GRID_PADDING + rows * STEP_Y - GRID_GAP,
  };
}

export function cardRect(layout: GridLayout, index: number): Rect {
  const row = Math.floor(index / layout.columns);
  const column = index % layout.columns;
  return { left: GRID_PADDING + column * STEP_X, top: GRID_PADDING + row * STEP_Y, ...CARD };
}

/**
 * The cards of every row that intersects `[top - overscan, top + height + overscan]`,
 * as an inclusive range; `null` with no pages.
 */
export function visibleCards(
  layout: GridLayout,
  top: number,
  height: number,
  overscan: number,
): { first: number; last: number } | null {
  if (layout.pageCount === 0) return null;
  const rowAt = (y: number) => Math.max(0, Math.min(layout.rows - 1, Math.floor((y - GRID_PADDING) / STEP_Y)));
  // A band ending in the gap under a row does not reach the next one.
  const firstRow = rowAt(top - overscan);
  const lastRow = rowAt(top + height + overscan - 1);
  return {
    first: firstRow * layout.columns,
    last: Math.min(layout.pageCount - 1, (lastRow + 1) * layout.columns - 1),
  };
}

/**
 * The slot under `point` (content coordinates): in front of the first card
 * of its row whose middle is right of the point. Outside the grid it clamps
 * to the nearest row and end; `null` with no pages.
 */
export function dropSlotAt(layout: GridLayout, point: Point): DropSlot | null {
  if (layout.pageCount === 0) return null;
  const row = Math.max(0, Math.min(layout.rows - 1, Math.floor((point.y - GRID_PADDING + GRID_GAP / 2) / STEP_Y)));
  const cardsInRow = Math.min(layout.columns, layout.pageCount - row * layout.columns);
  const column = Math.max(0, Math.min(cardsInRow, Math.ceil((point.x - GRID_PADDING - CARD.width / 2) / STEP_X)));
  return { index: row * layout.columns + column, row, column };
}

/** Where the drop marker of `slot` is drawn: a bar in the gap before card `column`. */
export function dropMarkerRect(slot: DropSlot): Rect {
  return {
    left: GRID_PADDING + slot.column * STEP_X - GRID_GAP / 2 - 2,
    top: GRID_PADDING + slot.row * STEP_Y,
    width: 4,
    height: CARD.height,
  };
}
