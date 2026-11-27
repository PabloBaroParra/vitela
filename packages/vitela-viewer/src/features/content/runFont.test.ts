import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { FontProgram } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { createRunFontRegistry, type RunFontAt } from './runFont';

/** A stand-in for the browser's `FontFace`: `reject` makes `load` fail, as the font sanitizer does. */
class FakeFace {
  static made: FakeFace[] = [];
  static reject = false;
  constructor(
    readonly family: string,
    readonly bytes: Uint8Array,
  ) {
    FakeFace.made.push(this);
  }
  load() {
    return FakeFace.reject ? Promise.reject(new Error('OTS parsing error')) : Promise.resolve(this);
  }
}

const fonts = { added: new Set<unknown>(), add: (face: unknown) => void fonts.added.add(face), delete: (face: unknown) => fonts.added.delete(face) };

beforeEach(() => {
  FakeFace.made = [];
  FakeFace.reject = false;
  fonts.added.clear();
  vi.stubGlobal('FontFace', FakeFace);
  Object.defineProperty(document, 'fonts', { value: fonts, configurable: true });
});
afterEach(() => {
  Reflect.deleteProperty(document, 'fonts');
});

const program = (...bytes: number[]): FontProgram => ({ format: 'trueType', bytes: new Uint8Array(bytes) });

/** Run `runId` on page `pageIndex` (page id `100 + pageIndex`), drawn in resource `F<runId>` unless told. */
const at = (pageIndex: number, runId: number, more: Partial<RunFontAt> = {}): RunFontAt => ({
  pageIndex,
  runId,
  pageId: 100 + pageIndex,
  fontName: `F${runId}`,
  baseGeneration: 0,
  ...more,
});

describe('createRunFontRegistry', () => {
  it('loads one face per font program, whatever run, page, resource name or base generation asks for it', async () => {
    // Runs 1 and 2 are drawn with the same program (say `F1` on the page and `F1` in a form XObject); run 3 with another.
    const programs: Record<number, FontProgram> = { 1: program(1, 2, 3), 2: program(1, 2, 3), 3: program(1, 2, 4) };
    const engine = fakeEngine({ textRunFontProgram: async (_document, _page, runId) => programs[runId]! });
    const registry = createRunFontRegistry(engine, 1);
    const first = await registry.load(at(0, 1));
    expect(first).not.toBeNull();
    expect(await registry.load(at(5, 2))).toBe(first);
    const other = await registry.load(at(0, 3));
    expect(other).not.toBeNull();
    expect(other).not.toBe(first);
    expect(FakeFace.made).toHaveLength(2);
    expect(fonts.added.size).toBe(2);
  });

  it('asks the worker for a program once per page, resource and base: the bytes are read only on a miss', async () => {
    const asked: number[] = [];
    const engine = fakeEngine({ textRunFontProgram: async (_document, _page, runId) => (asked.push(runId), program(1, 2, 3)) });
    const registry = createRunFontRegistry(engine, 1);
    const first = await registry.load(at(0, 1, { fontName: 'F1' }));
    // Another run drawn in the same resource on the same page, same base: no read.
    expect(await registry.load(at(0, 2, { fontName: 'F1' }))).toBe(first);
    expect(asked).toEqual([1]);
    // A checkpoint (new base) or another page asks again, and lands on the same face by its bytes.
    expect(await registry.load(at(0, 2, { fontName: 'F1', baseGeneration: 1 }))).toBe(first);
    expect(await registry.load(at(3, 4, { fontName: 'F1' }))).toBe(first);
    expect(asked).toEqual([1, 2, 4]);
    expect(FakeFace.made).toHaveLength(1);
  });

  it('keeps a font with no program by its key too, so a standard-14 run is asked about once', async () => {
    let calls = 0;
    const engine = fakeEngine({ textRunFontProgram: async () => (calls++, null) });
    const registry = createRunFontRegistry(engine, 1);
    expect(await registry.load(at(0, 1))).toBeNull();
    expect(await registry.load(at(0, 7, { fontName: 'F1' }))).toBeNull();
    expect(calls).toBe(1);
  });

  it('does not keep a read that failed: the next editor over the run asks again', async () => {
    let calls = 0;
    const engine = fakeEngine({
      textRunFontProgram: async () => {
        calls += 1;
        if (calls === 1) throw new Error('worker busy');
        return program(9);
      },
    });
    const registry = createRunFontRegistry(engine, 1);
    expect(await registry.load(at(0, 1))).toBeNull();
    expect(await registry.load(at(0, 1))).not.toBeNull();
    expect(calls).toBe(2);
  });

  it('draws a program the browser will not take in the fallback family, and does not try that program again', async () => {
    FakeFace.reject = true;
    const engine = fakeEngine({ textRunFontProgram: async () => program(7, 7) });
    const registry = createRunFontRegistry(engine, 1);
    expect(await registry.load(at(0, 1))).toBeNull();
    expect(await registry.load(at(0, 2))).toBeNull();
    expect(FakeFace.made).toHaveLength(1);
    expect(fonts.added.size).toBe(0);
  });

  it('takes none of a CFF or Type 1 program, nor a standard-14 font', async () => {
    const formats: Record<number, FontProgram | null> = { 1: { format: 'cff', bytes: new Uint8Array([1]) }, 2: { format: 'type1', bytes: new Uint8Array([2]) }, 3: null };
    const engine = fakeEngine({ textRunFontProgram: async (_document, _page, runId) => formats[runId] ?? null });
    const registry = createRunFontRegistry(engine, 1);
    for (const runId of [1, 2, 3]) expect(await registry.load(at(0, runId))).toBeNull();
    expect(FakeFace.made).toHaveLength(0);
  });

  it('removes every face it added from the page once disposed, and adds none that lands afterwards', async () => {
    let answer: (value: FontProgram) => void = () => {};
    const engine = fakeEngine({
      textRunFontProgram: async (_document, _page, runId) => (runId === 1 ? program(1) : new Promise<FontProgram>((resolve) => (answer = resolve))),
    });
    const registry = createRunFontRegistry(engine, 1);
    expect(await registry.load(at(0, 1))).not.toBeNull();
    const late = registry.load(at(0, 2));
    registry.dispose();
    expect(fonts.added.size).toBe(0);
    answer(program(2));
    expect(await late).toBeNull();
    expect(fonts.added.size).toBe(0);
  });
});
