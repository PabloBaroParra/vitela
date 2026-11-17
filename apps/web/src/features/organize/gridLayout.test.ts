import { describe, expect, it } from 'vitest';
import { CARD, GRID_GAP, GRID_PADDING, cardRect, dropSlotAt, layoutGrid, visibleCards } from './gridLayout';

const STEP_X = CARD.width + GRID_GAP;
const STEP_Y = CARD.height + GRID_GAP;
/** Room for exactly three cards a row. */
const WIDTH = 2 * GRID_PADDING + 3 * CARD.width + 2 * GRID_GAP;

describe('layoutGrid', () => {
  it('fits as many columns as the width holds, and at least one', () => {
    expect(layoutGrid(10, WIDTH).columns).toBe(3);
    expect(layoutGrid(10, WIDTH - 1).columns).toBe(2);
    expect(layoutGrid(10, 10).columns).toBe(1);
  });

  it('wraps the pages into rows and sizes the content to them', () => {
    const layout = layoutGrid(7, WIDTH);
    expect(layout.rows).toBe(3);
    expect(layout.height).toBe(2 * GRID_PADDING + 3 * CARD.height + 2 * GRID_GAP);
    expect(cardRect(layout, 4)).toEqual({ left: GRID_PADDING + STEP_X, top: GRID_PADDING + STEP_Y, ...CARD });
  });

  it('has room for its padding with no pages', () => {
    const layout = layoutGrid(0, WIDTH);
    expect(layout.rows).toBe(0);
    expect(layout.height).toBe(2 * GRID_PADDING);
  });
});

describe('visibleCards', () => {
  it('is the cards of every row touching the band', () => {
    const layout = layoutGrid(29, WIDTH);
    // Rows 1 and 2 show; no overscan.
    expect(visibleCards(layout, GRID_PADDING + STEP_Y, 2 * STEP_Y - GRID_GAP, 0)).toEqual({ first: 3, last: 8 });
    // The last row is partial.
    expect(visibleCards(layout, GRID_PADDING + 9 * STEP_Y, 1000, 0)).toEqual({ first: 27, last: 28 });
  });

  it('is nothing with no pages', () => {
    expect(visibleCards(layoutGrid(0, WIDTH), 0, 600, 600)).toBeNull();
  });
});

describe('dropSlotAt', () => {
  const layout = layoutGrid(7, WIDTH);
  const at = (column: number, row: number, dx = 0) => ({
    x: GRID_PADDING + column * STEP_X + CARD.width / 2 + dx,
    y: GRID_PADDING + row * STEP_Y + CARD.height / 2,
  });

  it('is before a card left of its middle, after it right of its middle', () => {
    expect(dropSlotAt(layout, at(1, 0, -1))).toEqual({ index: 1, row: 0, column: 1 });
    expect(dropSlotAt(layout, at(1, 0, 1))).toEqual({ index: 2, row: 0, column: 2 });
  });

  it('can be the end of a row and the end of the document', () => {
    expect(dropSlotAt(layout, at(2, 1, 1))).toEqual({ index: 6, row: 1, column: 3 });
    // Row 2 holds only page 7: past it is the end, whatever x says.
    expect(dropSlotAt(layout, at(2, 2))).toEqual({ index: 7, row: 2, column: 1 });
  });

  it('clamps a point outside the grid to its nearest slot', () => {
    expect(dropSlotAt(layout, { x: -50, y: -50 })).toEqual({ index: 0, row: 0, column: 0 });
    expect(dropSlotAt(layout, { x: 5000, y: 5000 })).toEqual({ index: 7, row: 2, column: 1 });
  });

  it('is nothing with no pages', () => {
    expect(dropSlotAt(layoutGrid(0, WIDTH), { x: 0, y: 0 })).toBeNull();
  });
});
