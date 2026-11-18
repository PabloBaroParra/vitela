import { describe, expect, it } from 'vitest';
import type { DocumentBlock } from '@vitela/core';
import { blockLabel, blockUnits, pageUnits } from './units';

const block = (start: number, count: number, source: DocumentBlock['source'] = { kind: 'base' }, part: number | null = null) => ({
  source,
  part,
  start,
  count,
});

/** [base 0..1] [a.pdf 2..4] [base 5]: six pages in three blocks. */
const BLOCKS: DocumentBlock[] = [
  block(0, 2, { kind: 'base' }, 1),
  block(2, 3, { kind: 'imported', id: 1 }),
  block(5, 1, { kind: 'base' }, 2),
];

describe('pageUnits', () => {
  it('makes every page its own unit', () => {
    const units = pageUnits(4);
    expect(units.count).toBe(4);
    expect(units.pageRun({ from: 1, count: 2 })).toEqual({ from: 1, count: 2 });
    expect(units.pageSlot(4)).toBe(4);
    expect(units.covering({ from: 2, count: 1 })).toEqual({ from: 2, count: 1 });
  });
});

describe('blockUnits', () => {
  const units = blockUnits(BLOCKS, 6);

  it('counts one unit per block', () => {
    expect(units.count).toBe(3);
  });

  it('turns a run of blocks into the run of their pages', () => {
    expect(units.pageRun({ from: 1, count: 1 })).toEqual({ from: 2, count: 3 });
    expect(units.pageRun({ from: 1, count: 2 })).toEqual({ from: 2, count: 4 });
  });

  it('turns a slot between blocks into a slot between pages', () => {
    expect(units.pageSlot(0)).toBe(0);
    expect(units.pageSlot(2)).toBe(5);
    expect(units.pageSlot(3)).toBe(6);
  });

  it('finds the blocks that hold a run of pages', () => {
    expect(units.covering({ from: 2, count: 3 })).toEqual({ from: 1, count: 1 });
    expect(units.covering({ from: 1, count: 2 })).toEqual({ from: 0, count: 2 });
    expect(units.covering({ from: 5, count: 1 })).toEqual({ from: 2, count: 1 });
  });
});

describe('blockLabel', () => {
  const names = new Map([[1, 'added.pdf']]);

  it('names the opened PDF, an imported one by the file it came from, and blank pages', () => {
    expect(blockLabel(block(0, 1), 'main.pdf', names)).toBe('main.pdf');
    expect(blockLabel(block(0, 1, { kind: 'imported', id: 1 }), 'main.pdf', names)).toBe('added.pdf');
    expect(blockLabel(block(0, 1, { kind: 'blank' }), 'main.pdf', names)).toBe('Blank pages');
  });

  it('numbers the parts of a PDF split across blocks', () => {
    expect(blockLabel(BLOCKS[2]!, 'main.pdf', names)).toBe('main.pdf (part 2)');
  });

  it('falls back to a generic name for an import it has no file name for', () => {
    expect(blockLabel(block(0, 1, { kind: 'imported', id: 9 }), 'main.pdf', names)).toBe('Added PDF');
  });
});
