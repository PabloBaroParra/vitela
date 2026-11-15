import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { EngineError } from '../../core/errors';
import type { Engine } from '../../core/types';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import type { WritableFileHandle } from './fileHandle';
import type { SaveOutcome } from './saveBytes';
import { useSave } from './useSave';

const document: OpenDocument = {
  name: 'a.pdf',
  handle: 7,
  generation: 1,
  info: { pageCount: 1, pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }], textExtractionAllowed: true },
};

const bytes = new Uint8Array([37, 80, 68, 70]);

/** An engine that saves `bytes`, recording each acknowledgement, unless `fail` throws. */
function savingEngine(fail?: (invalidate: boolean) => Error | undefined) {
  const asked: [number, boolean][] = [];
  const engine: Engine = fakeEngine({
    saveToBytes: async (handle, invalidate) => {
      asked.push([handle, invalidate]);
      const error = fail?.(invalidate);
      if (error) throw error;
      return bytes;
    },
  });
  return { engine, asked };
}

function deliveries() {
  const calls: { bytes: Uint8Array; name: string; handle: WritableFileHandle | undefined }[] = [];
  const outcome: SaveOutcome = { how: 'downloaded', name: 'a.pdf', fellBack: false };
  return { calls, deliver: async (request: (typeof calls)[number]) => (calls.push(request), outcome), outcome };
}

const signed = (invalidate: boolean) =>
  invalidate ? undefined : new EngineError('SignaturesWouldBeInvalidated', 'the document is signed');

describe('useSave', () => {
  it('starts idle and clean', () => {
    const { result } = renderHook(() => useSave(savingEngine().engine, document));
    expect(result.current.activity).toEqual({ kind: 'idle' });
    expect(result.current.dirty).toBe(false);
  });

  it('saves the open document and hands the bytes, name and file handle over', async () => {
    const { engine, asked } = savingEngine();
    const { calls, deliver, outcome } = deliveries();
    const fileHandle = { name: 'a.pdf' } as WritableFileHandle;
    const { result } = renderHook(() => useSave(engine, { ...document, fileHandle }, deliver));

    act(() => result.current.save());
    expect(result.current.activity).toEqual({ kind: 'saving' });
    await waitFor(() => expect(result.current.activity).toEqual({ kind: 'done', outcome }));

    expect(asked).toEqual([[7, false]]);
    expect(calls).toEqual([{ bytes, name: 'a.pdf', handle: fileHandle }]);
  });

  it('ignores a second Save while one is running', async () => {
    const { engine, asked } = savingEngine();
    const { deliver } = deliveries();
    const { result } = renderHook(() => useSave(engine, document, deliver));
    act(() => {
      result.current.save();
      result.current.save();
    });
    await waitFor(() => expect(result.current.activity.kind).toBe('done'));
    expect(asked).toHaveLength(1);
  });

  it('asks before breaking a signature, and saves only once the user agrees', async () => {
    const { engine, asked } = savingEngine(signed);
    const { calls, deliver } = deliveries();
    const { result } = renderHook(() => useSave(engine, document, deliver));

    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity).toEqual({ kind: 'confirmSignatures' }));
    expect(calls).toEqual([]);

    act(() => result.current.confirmSignatures());
    await waitFor(() => expect(result.current.activity.kind).toBe('done'));
    expect(asked).toEqual([
      [7, false],
      [7, true],
    ]);
    expect(calls).toHaveLength(1);
  });

  it('cancelling the signature question saves nothing', async () => {
    const { engine, asked } = savingEngine(signed);
    const { calls, deliver } = deliveries();
    const { result } = renderHook(() => useSave(engine, document, deliver));
    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity.kind).toBe('confirmSignatures'));

    act(() => result.current.cancelSignatures());
    expect(result.current.activity).toEqual({ kind: 'idle' });
    expect(asked).toEqual([[7, false]]);
    expect(calls).toEqual([]);
  });

  it('reports another engine failure with its message and delivers nothing', async () => {
    const { engine } = savingEngine(() => new EngineError('SaveFailed', 'the document could not be written'));
    const { calls, deliver } = deliveries();
    const { result } = renderHook(() => useSave(engine, document, deliver));
    act(() => result.current.save());
    await waitFor(() =>
      expect(result.current.activity).toEqual({ kind: 'failed', message: 'the document could not be written' }),
    );
    expect(calls).toEqual([]);
  });

  it('reports a delivery failure', async () => {
    const { engine } = savingEngine();
    const { result } = renderHook(() =>
      useSave(engine, document, async () => {
        throw new Error('no room left');
      }),
    );
    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity).toEqual({ kind: 'failed', message: 'no room left' }));
  });

  it('keeps the dirty mark of unsaved edits until a save succeeds', async () => {
    let failing = true;
    const { engine } = savingEngine(() => (failing ? new EngineError('SaveFailed', 'nope') : undefined));
    const { deliver } = deliveries();
    const { result } = renderHook(() => useSave(engine, document, deliver));

    act(() => result.current.markDirty());
    expect(result.current.dirty).toBe(true);

    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity.kind).toBe('failed'));
    expect(result.current.dirty).toBe(true);

    failing = false;
    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity.kind).toBe('done'));
    expect(result.current.dirty).toBe(false);
  });
});
