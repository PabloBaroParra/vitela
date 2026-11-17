import { describe, expect, it } from 'vitest';
import { clampSelection, contains, extendTo, runOf, selectOnly, selectRun } from './selection';

describe('selection', () => {
  it('is one contiguous run whichever way it was extended', () => {
    expect(runOf(selectOnly(3))).toEqual({ from: 3, count: 1 });
    expect(runOf(extendTo(selectOnly(3), 6))).toEqual({ from: 3, count: 4 });
    expect(runOf(extendTo(selectOnly(3), 1))).toEqual({ from: 1, count: 3 });
  });

  it('keeps its anchor when extended again', () => {
    const selection = extendTo(extendTo(selectOnly(4), 7), 2);
    expect(selection).toEqual({ anchor: 4, focus: 2 });
  });

  it('says which pages it holds', () => {
    const selection = extendTo(selectOnly(2), 4);
    expect([1, 2, 3, 4, 5].map((index) => contains(selection, index))).toEqual([false, true, true, true, false]);
  });

  it('selects a run with the focus on its last page', () => {
    expect(selectRun({ from: 2, count: 3 })).toEqual({ anchor: 2, focus: 4 });
  });

  it('shrinks to the pages that still exist, and to nothing with no pages', () => {
    expect(clampSelection({ anchor: 2, focus: 9 }, 5)).toEqual({ anchor: 2, focus: 4 });
    expect(clampSelection({ anchor: 7, focus: 9 }, 5)).toEqual({ anchor: 4, focus: 4 });
    expect(clampSelection({ anchor: 0, focus: 0 }, 0)).toBeNull();
    expect(clampSelection(null, 3)).toBeNull();
  });
});
