import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { EngineError } from '@vitela/core';
import type { PreparedImport } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { useImport, type PreparedSource } from './useImport';

const pdf = (name: string, bytes: number[] = [1]) => new File([new Uint8Array(bytes)], name, { type: 'application/pdf' });

/** A file whose first byte is 0xee is locked under "secret". */
function importEngine() {
  const prepared: [number, string | undefined][] = [];
  const discarded: number[] = [];
  let next = 1;
  const engine = fakeEngine({
    prepareImport: async (bytes, password): Promise<PreparedImport> => {
      prepared.push([bytes[0]!, password]);
      if (bytes[0] === 0xee && password !== 'secret') {
        throw new EngineError(password === undefined ? 'PasswordRequired' : 'WrongPassword', 'password needed');
      }
      if (bytes[0] === 0xbb) throw new EngineError('UnsupportedOperation', 'the PDF does not permit copying its pages');
      return { id: next++, pageCount: bytes.length, warnings: [] };
    },
    discardImport: async (id) => void discarded.push(id),
  });
  return { engine, prepared, discarded };
}

function setup(importAll: (sources: readonly PreparedSource[]) => Promise<boolean> = async () => true) {
  const fake = importEngine();
  const imported = vi.fn(importAll);
  const hook = renderHook(() => useImport(fake.engine, imported));
  return { ...fake, imported, ...hook };
}

describe('useImport', () => {
  it('prepares every picked file in order and imports them all at once', async () => {
    const { result, imported, prepared } = setup();
    await act(() => result.current.add([pdf('a.pdf', [1, 1]), pdf('b.pdf')]));
    expect(prepared).toEqual([
      [1, undefined],
      [1, undefined],
    ]);
    expect(imported).toHaveBeenCalledTimes(1);
    expect(imported.mock.calls[0]![0]).toEqual([
      { id: 1, pageCount: 2, warnings: [], name: 'a.pdf' },
      { id: 2, pageCount: 1, warnings: [], name: 'b.pdf' },
    ]);
    expect(result.current.activity).toEqual({ kind: 'added', message: 'Added 3 pages from 2 PDFs.' });
  });

  it('asks a locked file for its password, says when it was wrong, and goes on with the right one', async () => {
    const { result, imported, prepared } = setup();
    let adding!: Promise<void>;
    act(() => {
      adding = result.current.add([pdf('locked.pdf', [0xee])]);
    });
    await waitFor(() =>
      expect(result.current.activity).toEqual({ kind: 'password', name: 'locked.pdf', wrong: false, checking: false }),
    );
    act(() => result.current.submitPassword('nope'));
    await waitFor(() =>
      expect(result.current.activity).toEqual({ kind: 'password', name: 'locked.pdf', wrong: true, checking: false }),
    );
    await act(async () => {
      result.current.submitPassword('secret');
      await adding;
    });
    expect(prepared.map(([, password]) => password)).toEqual([undefined, 'nope', 'secret']);
    expect(imported).toHaveBeenCalledTimes(1);
    expect(result.current.activity).toEqual({ kind: 'added', message: 'Added 1 page from locked.pdf.' });
  });

  it('gives up the whole pick when a password is cancelled, discarding what it prepared', async () => {
    const { result, imported, discarded } = setup();
    let adding!: Promise<void>;
    act(() => {
      adding = result.current.add([pdf('a.pdf'), pdf('locked.pdf', [0xee])]);
    });
    await waitFor(() => expect(result.current.activity.kind).toBe('password'));
    await act(async () => {
      result.current.cancelPassword();
      await adding;
    });
    expect(imported).not.toHaveBeenCalled();
    expect(discarded).toEqual([1]);
    expect(result.current.activity).toEqual({ kind: 'idle' });
  });

  it('stops at a file the core refuses, saying which and why', async () => {
    const { result, imported, discarded } = setup();
    await act(() => result.current.add([pdf('a.pdf'), pdf('no-copy.pdf', [0xbb])]));
    expect(imported).not.toHaveBeenCalled();
    expect(discarded).toEqual([1]);
    expect(result.current.activity).toEqual({
      kind: 'failed',
      message: 'Could not add no-copy.pdf: the PDF does not permit copying its pages',
    });
  });

  it('discards the prepared files when the document does not take them', async () => {
    const { result, discarded } = setup(async () => false);
    await act(() => result.current.add([pdf('a.pdf')]));
    expect(discarded).toEqual([1]);
    expect(result.current.activity).toEqual({ kind: 'idle' });
  });

  it('runs one pick at a time', async () => {
    let release!: (taken: boolean) => void;
    const { result, imported } = setup(() => new Promise((resolve) => (release = resolve)));
    let first!: Promise<void>;
    act(() => {
      first = result.current.add([pdf('a.pdf')]);
    });
    await waitFor(() => expect(result.current.activity.kind).toBe('importing'));
    expect(result.current.busy).toBe(true);
    await act(() => result.current.add([pdf('b.pdf')]));
    await act(async () => {
      release(true);
      await first;
    });
    expect(imported).toHaveBeenCalledTimes(1);
    expect(result.current.busy).toBe(false);
  });

  it('abandons the pick when the screen goes away while it asks for a password', async () => {
    const { result, imported, discarded, unmount } = setup();
    let adding!: Promise<void>;
    act(() => {
      adding = result.current.add([pdf('a.pdf'), pdf('locked.pdf', [0xee])]);
    });
    await waitFor(() => expect(result.current.activity.kind).toBe('password'));
    unmount();
    await adding;
    expect(imported).not.toHaveBeenCalled();
    expect(discarded).toEqual([1]);
  });

  it('mentions what the pages could not bring across', async () => {
    const fake = importEngine();
    const engine = { ...fake.engine, prepareImport: async () => ({ id: 1, pageCount: 1, warnings: ['a link was dropped'] }) };
    const { result } = renderHook(() => useImport(engine, async () => true));
    await act(() => result.current.add([pdf('a.pdf')]));
    expect(result.current.activity).toEqual({
      kind: 'added',
      message: 'Added 1 page from a.pdf. Not everything came across: a link was dropped.',
    });
  });
});
