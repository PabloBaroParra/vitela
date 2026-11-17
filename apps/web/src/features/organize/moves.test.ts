import { describe, expect, it } from 'vitest';
import { moveBy, moveToSlot, runAfterMove } from './moves';

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
