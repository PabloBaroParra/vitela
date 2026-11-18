import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { EngineError } from '@vitela/core';
import type { DocumentInfo, EditedDocument, PageEdit } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import { useOrganize } from './useOrganize';

function info(pageCount: number, documentAssemblyAllowed = true): DocumentInfo {
  return {
    pageCount,
    pages: Array(pageCount).fill({ widthPt: 612, heightPt: 792, rotationDegrees: 0 }),
    pageIds: Array.from({ length: pageCount }, (_, index) => index),
    textExtractionAllowed: true,
    documentAssemblyAllowed,
    blocks: [{ source: { kind: 'base' }, part: null, start: 0, count: pageCount }],
    importRefusal: null,
  };
}

const doc = (overrides: Partial<OpenDocument> = {}): OpenDocument => ({
  name: 'a.pdf',
  info: info(3),
  handle: 4,
  generation: 1,
  ...overrides,
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe('useOrganize', () => {
  it('applies an edit to the document it names and reports the edited document', async () => {
    const calls: [number, PageEdit][] = [];
    const edited: EditedDocument = { info: info(2), canUndo: true, canRedo: false };
    const engine = fakeEngine({ applyEdit: async (handle, edit) => (calls.push([handle, edit]), edited) });
    const reported: EditedDocument[] = [];
    const { result } = renderHook(() => useOrganize(engine, doc(), (e) => reported.push(e)));

    let answer: EditedDocument | null = null;
    await act(async () => {
      answer = await result.current.apply({ kind: 'remove', from: 0, count: 1 });
    });
    expect(calls).toEqual([[4, { kind: 'remove', from: 0, count: 1 }]]);
    expect(answer).toBe(edited);
    expect(reported).toEqual([edited]);
    expect(result.current.pending).toBe(false);
  });

  it('runs one edit at a time: another asked for while one is in flight is dropped', async () => {
    const first = deferred<EditedDocument>();
    let calls = 0;
    const engine = fakeEngine({ applyEdit: () => (calls++, first.promise) });
    const { result } = renderHook(() => useOrganize(engine, doc(), () => {}));

    let running!: Promise<EditedDocument | null>;
    act(() => {
      running = result.current.apply({ kind: 'rotate', from: 0, count: 1, degrees: 90 });
    });
    expect(result.current.pending).toBe(true);
    let dropped: EditedDocument | null | undefined;
    await act(async () => {
      dropped = await result.current.apply({ kind: 'rotate', from: 0, count: 1, degrees: 90 });
    });
    expect(dropped).toBeNull();
    expect(calls).toBe(1);

    await act(async () => {
      first.resolve({ info: info(3), canUndo: true, canRedo: false });
      await running;
    });
    expect(result.current.pending).toBe(false);
  });

  it('undoes and redoes only when the history says there is something to step to', async () => {
    const steps: string[] = [];
    const edited: EditedDocument = { info: info(3), canUndo: false, canRedo: true };
    const engine = fakeEngine({
      undo: async () => (steps.push('undo'), edited),
      redo: async () => (steps.push('redo'), edited),
    });
    const fresh = renderHook(() => useOrganize(engine, doc(), () => {}));
    await act(async () => {
      expect(await fresh.result.current.undo()).toBeNull();
      expect(await fresh.result.current.redo()).toBeNull();
    });
    expect(steps).toEqual([]);

    const edited2 = renderHook(() => useOrganize(engine, doc({ canUndo: true, canRedo: true }), () => {}));
    await act(async () => {
      await edited2.result.current.undo();
      await edited2.result.current.redo();
    });
    expect(steps).toEqual(['undo', 'redo']);
  });

  it('imports prepared sources after the last page and reports the edited document', async () => {
    const calls: [number, readonly number[], number][] = [];
    const imported = { info: info(5), canUndo: true, canRedo: false, sourceIds: [1] };
    const engine = fakeEngine({ importPrepared: async (...args) => (calls.push(args), imported) });
    const reported: EditedDocument[] = [];
    const { result } = renderHook(() => useOrganize(engine, doc(), (e) => reported.push(e)));
    await act(async () => {
      expect(await result.current.importPrepared([7, 8])).toBe(imported);
    });
    expect(calls).toEqual([[4, [7, 8], 3]]);
    expect(reported).toEqual([imported]);
  });

  it('runs an import asked for while an edit is in flight once that edit lands, after the pages it left', async () => {
    const edit = deferred<EditedDocument>();
    const steps: string[] = [];
    const imported = { info: info(4), canUndo: true, canRedo: false, sourceIds: [1] };
    const engine = fakeEngine({
      applyEdit: () => (steps.push('edit'), edit.promise),
      importPrepared: async (_handle, _ids, index) => (steps.push(`import at ${index}`), imported),
    });
    const { result } = renderHook(() => useOrganize(engine, doc(), () => {}));

    let editing!: Promise<EditedDocument | null>;
    let importing!: Promise<unknown>;
    act(() => {
      editing = result.current.apply({ kind: 'remove', from: 0, count: 1 });
      importing = result.current.importPrepared([7]);
    });
    await act(async () => {
      edit.resolve({ info: info(2), canUndo: true, canRedo: false });
      await editing;
      expect(await importing).toBe(imported);
    });
    expect(steps).toEqual(['edit', 'import at 2']);
    expect(result.current.pending).toBe(false);
  });

  it('keeps the failure of the edit an import waited for on show', async () => {
    const edit = deferred<EditedDocument>();
    const engine = fakeEngine({
      applyEdit: () => edit.promise,
      importPrepared: async () => ({ info: info(4), canUndo: true, canRedo: false, sourceIds: [1] }),
    });
    const { result } = renderHook(() => useOrganize(engine, doc(), () => {}));
    let importing!: Promise<unknown>;
    act(() => {
      void result.current.apply({ kind: 'remove', from: 0, count: 1 });
      importing = result.current.importPrepared([7]);
    });
    await act(async () => {
      edit.reject(new EngineError('UnsupportedOperation', 'cannot rewrite this document'));
      await importing;
    });
    expect(result.current.failure).toBe('cannot rewrite this document');
  });

  it('turns an import the core refuses into a failure and answers null', async () => {
    const engine = fakeEngine({
      importPrepared: async () => {
        throw new EngineError('EditNotPermitted', 'this document does not permit changing its content');
      },
    });
    const { result } = renderHook(() => useOrganize(engine, doc(), () => {}));
    await act(async () => {
      expect(await result.current.importPrepared([1])).toBeNull();
    });
    expect(result.current.failure).toBe('this document does not permit changing its content');
    expect(result.current.allowed).toBe(false);
  });

  it('is blocked when the document withholds the assembly permission', () => {
    const { result } = renderHook(() => useOrganize(fakeEngine(), doc({ info: info(3, false) }), () => {}));
    expect(result.current.allowed).toBe(false);
  });

  it('turns an EditNotPermitted refusal into blocked, and shows the reason', async () => {
    const engine = fakeEngine({
      applyEdit: async () => {
        throw new EngineError('EditNotPermitted', 'the document does not permit page assembly');
      },
    });
    const reported: EditedDocument[] = [];
    const { result } = renderHook(() => useOrganize(engine, doc(), (e) => reported.push(e)));
    await act(async () => {
      expect(await result.current.apply({ kind: 'remove', from: 0, count: 1 })).toBeNull();
    });
    expect(result.current.allowed).toBe(false);
    expect(result.current.failure).toBe('the document does not permit page assembly');
    expect(reported).toEqual([]);
  });

  it('shows any other refusal and stays allowed; the next edit clears it', async () => {
    let fail = true;
    const engine = fakeEngine({
      applyEdit: async () => {
        if (fail) throw new EngineError('UnsupportedOperation', 'cannot rewrite this document');
        return { info: info(3), canUndo: true, canRedo: false };
      },
    });
    const { result } = renderHook(() => useOrganize(engine, doc(), () => {}));
    await act(async () => void (await result.current.apply({ kind: 'remove', from: 0, count: 1 })));
    expect(result.current.failure).toBe('cannot rewrite this document');
    expect(result.current.allowed).toBe(true);

    fail = false;
    await act(async () => void (await result.current.apply({ kind: 'remove', from: 0, count: 1 })));
    expect(result.current.failure).toBeNull();
  });

  it('still reports an edit that lands after the screen went away: the core has it', async () => {
    const pending = deferred<EditedDocument>();
    const engine = fakeEngine({ applyEdit: () => pending.promise });
    const reported: EditedDocument[] = [];
    const { result, unmount } = renderHook(() => useOrganize(engine, doc(), (e) => reported.push(e)));
    let running!: Promise<EditedDocument | null>;
    act(() => {
      running = result.current.apply({ kind: 'remove', from: 0, count: 1 });
    });
    unmount();
    const edited = { info: info(2), canUndo: true, canRedo: false };
    pending.resolve(edited);
    await running;
    expect(reported).toEqual([edited]);
  });
});
