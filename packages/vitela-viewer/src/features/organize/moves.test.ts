import { describe, expect, it } from 'vitest';
import { moveBy, moveToSlot, moveUnitsBy, moveUnitsToSlot, runAfterMove } from './moves';
import { blockUnits, pageUnits } from './units';

describe('moveToSlot', () => {
  it('moves a run in front of an earlier page', () => {
    expect(moveToSlot({ from: 3, count: 2 }, 1)).toEqual({ kind: 'move', from: 3, count: 2, to: 1 });
  });

  it('moves a run after a later page, counting the run out of the way', () => {
    // [a b c d e]: dropping b+c before e (slot 4) leaves them at 2..3.
    expect(moveToSlot({ from: 1, count: 2 }, 4)).toEqual({ kind: 'move', from: 1, count: 2, to: 2 });
    // Past the last page.
    expect(moveToSlot({ from: 0, count: 1 }, 5)).toEqual({ kind: 'move', from: 0, count: 1, to: 4 });
  });

  it('is no move when dropped on or right next to itself', () => {
    expect(moveToSlot({ from: 2, count: 2 }, 2)).toBeNull();
    expect(moveToSlot({ from: 2, count: 2 }, 3)).toBeNull();
    expect(moveToSlot({ from: 2, count: 2 }, 4)).toBeNull();
  });
});

describe('moveBy', () => {
  it('moves a run one page earlier or later', () => {
    expect(moveBy({ from: 2, count: 2 }, -1, 6)).toEqual({ kind: 'move', from: 2, count: 2, to: 1 });
    expect(moveBy({ from: 2, count: 2 }, 1, 6)).toEqual({ kind: 'move', from: 2, count: 2, to: 3 });
  });

  it('stops at either end', () => {
    expect(moveBy({ from: 0, count: 2 }, -1, 6)).toBeNull();
    expect(moveBy({ from: 4, count: 2 }, 1, 6)).toBeNull();
    expect(moveBy({ from: 1, count: 1 }, -4, 6)).toEqual({ kind: 'move', from: 1, count: 1, to: 0 });
  });
});

describe('runAfterMove', () => {
  it('is where the moved pages ended up', () => {
    expect(runAfterMove({ kind: 'move', from: 1, count: 2, to: 3 })).toEqual({ from: 3, count: 2 });
  });
});

describe('moveUnitsToSlot / moveUnitsBy', () => {
  // [base 0..1] [a.pdf 2..4] [base 5]
  const units = blockUnits(
    [
      { source: { kind: 'base' }, part: 1, start: 0, count: 2 },
      { source: { kind: 'imported', id: 1 }, part: null, start: 2, count: 3 },
      { source: { kind: 'base' }, part: 2, start: 5, count: 1 },
    ],
    6,
  );

  it('moves a whole block in front of another, as one page move', () => {
    expect(moveUnitsToSlot(units, { from: 1, count: 1 }, 0)).toEqual({ kind: 'move', from: 2, count: 3, to: 0 });
    expect(moveUnitsToSlot(units, { from: 0, count: 1 }, 3)).toEqual({ kind: 'move', from: 0, count: 2, to: 4 });
    expect(moveUnitsToSlot(units, { from: 1, count: 1 }, 2)).toBeNull();
  });

  it('moves a block past its neighbour, whatever their page counts', () => {
    expect(moveUnitsBy(units, { from: 1, count: 1 }, -1)).toEqual({ kind: 'move', from: 2, count: 3, to: 0 });
    expect(moveUnitsBy(units, { from: 1, count: 1 }, 1)).toEqual({ kind: 'move', from: 2, count: 3, to: 3 });
    expect(moveUnitsBy(units, { from: 2, count: 1 }, 1)).toBeNull();
    expect(moveUnitsBy(units, { from: 0, count: 1 }, -4)).toBeNull();
  });

  it('agrees with moveBy for pages', () => {
    const pages = pageUnits(6);
    for (const [from, count, delta] of [
      [2, 2, -1],
      [2, 2, 1],
      [0, 2, -1],
      [4, 2, 1],
      [1, 1, -4],
      [1, 1, 9],
    ] as const) {
      expect(moveUnitsBy(pages, { from, count }, delta)).toEqual(moveBy({ from, count }, delta, 6));
    }
  });
});
