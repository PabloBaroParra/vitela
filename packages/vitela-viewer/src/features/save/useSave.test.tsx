import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { EngineError } from '@vitela/core';
import type { Engine } from '@vitela/core';
import type { WritableFileHandle } from '../../platform/fileHandle';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import { type DeliverRequest, type Delivered, type Delivery, type Destination, type SaveIntent } from './delivery';
import type { SaveOutcome } from './outcome';
import { useSave } from './useSave';

const document: OpenDocument = {
  name: 'a.pdf',
  handle: 7,
  generation: 1,
  info: {
    pageCount: 1,
    pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }],
    pageIds: [0],
    textExtractionAllowed: true,
    documentAssemblyAllowed: true,
    blocks: [],
    importRefusal: null,
    annotationEditingAllowed: true,
    formFillAllowed: true,
  },
};

const bytes = new Uint8Array([37, 80, 68, 70]);

/** Everything the save did, in order, across the engine and the delivery. */
type Log = string[];

/** An engine that saves `bytes`, recording each acknowledgement, unless `fail` throws. */
function savingEngine(log: Log = [], fail?: (invalidate: boolean) => Error | undefined) {
  const asked: [number, boolean][] = [];
  const engine: Engine = fakeEngine({
    saveToBytes: async (handle, invalidate) => {
      log.push('engine');
      asked.push([handle, invalidate]);
      const error = fail?.(invalidate);
      if (error) throw error;
      return bytes;
    },
  });
  return { engine, asked };
}

const outcome: SaveOutcome = { how: 'downloaded', name: 'a.pdf', fellBack: null };
const wrote: SaveOutcome = { how: 'wrote', name: 'a.pdf' };

function deliveries(
  log: Log = [],
  destination: Destination | null = { kind: 'overwrite', handle: undefined, permission: 'granted' },
  result: Delivered = outcome,
) {
  const prepared: { intent: SaveIntent; name: string; fileHandle: WritableFileHandle | undefined }[] = [];
  const calls: DeliverRequest[] = [];
  const delivery: Delivery = {
    prepare: async (intent, source) => {
      log.push(`prepare ${intent}`);
      prepared.push({ intent, name: source.name, fileHandle: source.fileHandle });
      return destination;
    },
    deliver: async (request) => (log.push('deliver'), calls.push(request), result),
  };
  return { prepared, calls, delivery };
}

const signed = (invalidate: boolean) =>
  invalidate ? undefined : new EngineError('SignaturesWouldBeInvalidated', 'the document is signed');

describe('useSave', () => {
  it('starts idle and clean', () => {
    const { result } = renderHook(() => useSave(savingEngine().engine, document));
    expect(result.current.activity).toEqual({ kind: 'idle' });
    expect(result.current.dirty).toBe(false);
  });

  it('prepares the destination BEFORE the engine saves, then hands over the bytes, name and destination', async () => {
    const log: Log = [];
    const { engine, asked } = savingEngine(log);
    const destination: Destination = { kind: 'overwrite', handle: undefined, permission: 'denied' };
    const { prepared, calls, delivery } = deliveries(log, destination);
    const fileHandle = { name: 'a.pdf' } as WritableFileHandle;
    const { result } = renderHook(() => useSave(engine, { ...document, fileHandle }, delivery));

    act(() => result.current.save());
    expect(result.current.activity).toEqual({ kind: 'saving' });
    await waitFor(() => expect(result.current.activity).toEqual({ kind: 'done', outcome }));

    expect(log).toEqual(['prepare overwrite', 'engine', 'deliver']);
    expect(prepared).toEqual([{ intent: 'overwrite', name: 'a.pdf', fileHandle }]);
    expect(asked).toEqual([[7, false]]);
    expect(calls).toEqual([{ bytes, name: 'a.pdf', destination }]);
  });

  it('waits for the edits already asked for before the engine saves, and counts them as saved', async () => {
    const log: Log = [];
    const { engine } = savingEngine(log);
    const { delivery } = deliveries(log, undefined, wrote);
    const { result } = renderHook(() => useSave(engine, document, delivery));
    let land: () => void = () => {};
    const edits = new Promise<void>((resolve) => (land = resolve));
    act(() => void result.current.waitForEdits(() => edits));

    act(() => result.current.save());
    await waitFor(() => expect(log).toEqual(['prepare overwrite']));
    // The queued edit lands (and flags the document) only now.
    await act(async () => {
      result.current.markDirty();
      land();
    });
    await waitFor(() => expect(result.current.activity.kind).toBe('done'));
    expect(log).toEqual(['prepare overwrite', 'engine', 'deliver']);
    expect(result.current.dirty).toBe(false);
  });

  it('stops waiting for the edits of a screen that has gone', async () => {
    const log: Log = [];
    const { engine } = savingEngine(log);
    const { delivery } = deliveries(log);
    const { result } = renderHook(() => useSave(engine, document, delivery));
    let stop: () => void = () => {};
    act(() => void (stop = result.current.waitForEdits(() => new Promise(() => {}))));
    act(() => stop());

    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity).toEqual({ kind: 'done', outcome }));
  });

  it('saves nothing, and keeps the edits marked, when the user dismisses a dialog', async () => {
    const log: Log = [];
    const { engine } = savingEngine(log);
    const { delivery } = deliveries(log, null);
    const { result } = renderHook(() => useSave(engine, document, delivery));
    act(() => result.current.markDirty());

    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity).toEqual({ kind: 'done', outcome: { how: 'cancelled' } }));
    expect(log).toEqual(['prepare overwrite']);
    expect(result.current.dirty).toBe(true);
  });

  it('ignores a second Save while one is running', async () => {
    const { engine, asked } = savingEngine();
    const { delivery } = deliveries();
    const { result } = renderHook(() => useSave(engine, document, delivery));
    act(() => {
      result.current.save();
      result.current.save();
    });
    await waitFor(() => expect(result.current.activity.kind).toBe('done'));
    expect(asked).toHaveLength(1);
  });

  it('asks before breaking a signature, and saves over the file only once the user agrees', async () => {
    const { engine, asked } = savingEngine([], signed);
    const { calls, delivery } = deliveries();
    const { result } = renderHook(() => useSave(engine, document, delivery));

    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity).toEqual({ kind: 'confirmSignatures', forCopy: false }));
    expect(calls).toEqual([]);

    act(() => result.current.confirmSignatures());
    await waitFor(() => expect(result.current.activity.kind).toBe('done'));
    expect(asked).toEqual([
      [7, false],
      [7, true],
    ]);
    expect(calls).toHaveLength(1);
  });

  it('"save a copy" from the signature question prepares a COPY first, then saves with the acknowledgement', async () => {
    const log: Log = [];
    const { engine, asked } = savingEngine(log, signed);
    const copy: Destination = { kind: 'copy' };
    const { prepared, calls, delivery } = deliveries(log, copy);
    const { result } = renderHook(() => useSave(engine, document, delivery));
    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity.kind).toBe('confirmSignatures'));
    log.length = 0;

    act(() => result.current.saveSignedCopy());
    await waitFor(() => expect(result.current.activity.kind).toBe('done'));

    expect(log).toEqual(['prepare copy', 'engine', 'deliver']);
    expect(prepared.at(-1)?.intent).toBe('copy');
    expect(asked.at(-1)).toEqual([7, true]);
    expect(calls).toEqual([{ bytes, name: 'a.pdf', destination: copy }]);
  });

  it('cancelling the signature question saves nothing', async () => {
    const { engine, asked } = savingEngine([], signed);
    const { calls, delivery } = deliveries();
    const { result } = renderHook(() => useSave(engine, document, delivery));
    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity.kind).toBe('confirmSignatures'));

    act(() => result.current.cancelSignatures());
    expect(result.current.activity).toEqual({ kind: 'idle' });
    expect(asked).toEqual([[7, false]]);
    expect(calls).toEqual([]);
  });

  it('reports another engine failure with its message and delivers nothing', async () => {
    const { engine } = savingEngine([], () => new EngineError('SaveFailed', 'the document could not be written'));
    const { calls, delivery } = deliveries();
    const { result } = renderHook(() => useSave(engine, document, delivery));
    act(() => result.current.save());
    await waitFor(() =>
      expect(result.current.activity).toEqual({ kind: 'failed', message: 'the document could not be written' }),
    );
    expect(calls).toEqual([]);
  });

  it('reports a delivery failure', async () => {
    const { engine } = savingEngine();
    const failing: Delivery = {
      ...deliveries().delivery,
      deliver: async () => {
        throw new Error('no room left');
      },
    };
    const { result } = renderHook(() => useSave(engine, document, failing));
    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity).toEqual({ kind: 'failed', message: 'no room left' }));
  });

  it('keeps the dirty mark of unsaved edits until a save succeeds', async () => {
    let failing = true;
    const { engine } = savingEngine([], () => (failing ? new EngineError('SaveFailed', 'nope') : undefined));
    const { delivery } = deliveries(undefined, undefined, wrote);
    const { result } = renderHook(() => useSave(engine, document, delivery));

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

  it('an edit after a successful save replaces "saved" with unsaved changes', async () => {
    const { engine } = savingEngine();
    const { delivery } = deliveries();
    const { result } = renderHook(() => useSave(engine, document, delivery));
    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity.kind).toBe('done'));

    act(() => result.current.markDirty());
    expect(result.current.dirty).toBe(true);
    expect(result.current.activity).toEqual({ kind: 'idle' });
  });

  it('starts clean for the next document, and a save that outlives its document reports nothing', async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => (release = resolve));
    const engine: Engine = fakeEngine({ saveToBytes: async () => (await gate, bytes) });
    const { delivery } = deliveries();
    const { result, rerender } = renderHook(({ doc }) => useSave(engine, doc, delivery), {
      initialProps: { doc: document },
    });
    act(() => result.current.markDirty());
    act(() => result.current.save());
    expect(result.current.activity.kind).toBe('saving');

    rerender({ doc: { ...document, generation: 2, handle: 8 } });
    expect(result.current.activity).toEqual({ kind: 'idle' });
    expect(result.current.dirty).toBe(false);

    await act(async () => release());
    expect(result.current.activity).toEqual({ kind: 'idle' });
  });

  it('a save still pending for an old document does not block Save on the new one', async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => (release = resolve));
    const log: Log = [];
    const asked: number[] = [];
    const engine: Engine = fakeEngine({
      saveToBytes: async (handle) => {
        asked.push(handle);
        if (handle === 7) await gate;
        return bytes;
      },
    });
    const { delivery, calls } = deliveries(log);
    const { result, rerender } = renderHook(({ doc }) => useSave(engine, doc, delivery), { initialProps: { doc: document } });
    act(() => result.current.save());
    rerender({ doc: { ...document, generation: 2, handle: 8, name: 'b.pdf' } });

    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity.kind).toBe('done'));
    expect(asked).toEqual([7, 8]);

    // The stale save finishes: it delivers nothing and changes nothing.
    await act(async () => release());
    expect(calls.map((c) => c.name)).toEqual(['b.pdf']);
    expect(result.current.activity.kind).toBe('done');
  });

  describe('what a save leaves unsaved', () => {
    it.each([
      ['a download', outcome],
      ['a download that fell back', { how: 'downloaded', name: 'a.pdf', fellBack: 'write' } as SaveOutcome],
      ['a copy', { how: 'copied', name: 'a (copy).pdf' } as SaveOutcome],
    ])('%s does not clear the unsaved changes: the opened file lacks the edits', async (_label, delivered) => {
      const { engine } = savingEngine();
      const { delivery } = deliveries(undefined, undefined, delivered);
      const { result } = renderHook(() => useSave(engine, document, delivery));
      act(() => result.current.markDirty());

      act(() => result.current.save());
      await waitFor(() => expect(result.current.activity).toEqual({ kind: 'done', outcome: delivered }));
      expect(result.current.dirty).toBe(true);
    });

    it('writing over the opened file clears them', async () => {
      const { engine } = savingEngine();
      const { delivery } = deliveries(undefined, undefined, wrote);
      const { result } = renderHook(() => useSave(engine, document, delivery));
      act(() => result.current.markDirty());

      act(() => result.current.save());
      await waitFor(() => expect(result.current.activity.kind).toBe('done'));
      expect(result.current.dirty).toBe(false);
    });

    it('handing the bytes to the host page clears them', async () => {
      const { engine } = savingEngine();
      const { delivery } = deliveries(undefined, undefined, { how: 'delivered', name: 'a.pdf' });
      const { result } = renderHook(() => useSave(engine, document, delivery));
      act(() => result.current.markDirty());

      act(() => result.current.save());
      await waitFor(() => expect(result.current.activity.kind).toBe('done'));
      expect(result.current.dirty).toBe(false);
    });

    it('an edit made while the save runs stays unsaved even when the save wrote the file', async () => {
      let release: () => void = () => {};
      const gate = new Promise<void>((resolve) => (release = resolve));
      const engine: Engine = fakeEngine({ saveToBytes: async () => (await gate, bytes) });
      const { delivery } = deliveries(undefined, undefined, wrote);
      const { result } = renderHook(() => useSave(engine, document, delivery));
      act(() => result.current.markDirty());
      act(() => result.current.save());
      await waitFor(() => expect(result.current.activity.kind).toBe('saving'));

      act(() => result.current.markDirty());
      await act(async () => release());

      await waitFor(() => expect(result.current.activity.kind).toBe('done'));
      expect(result.current.dirty).toBe(true);
    });
  });

  describe('the opened file changed on disk', () => {
    const changed: Destination = { kind: 'changedOnDisk' };
    const copy: Destination = { kind: 'copy' };
    const overwrite: Destination = { kind: 'overwrite', handle: undefined, permission: 'granted' };

    /** A delivery that answers "changed" to a plain overwrite and settles every other intent. */
    function changedDelivery(log: Log) {
      const { prepared, calls, delivery } = deliveries(log);
      const answering: Delivery = {
        ...delivery,
        prepare: async (intent, source) => {
          await delivery.prepare(intent, source);
          return intent === 'overwrite' ? changed : intent === 'copy' ? copy : overwrite;
        },
      };
      return { prepared, calls, delivery: answering };
    }

    it('asks, and neither runs the engine nor delivers, when prepare reports a change', async () => {
      const log: Log = [];
      const { engine, asked } = savingEngine(log);
      const { calls, delivery } = changedDelivery(log);
      const { result } = renderHook(() => useSave(engine, document, delivery));
      act(() => result.current.markDirty());

      act(() => result.current.save());
      await waitFor(() => expect(result.current.activity).toEqual({ kind: 'confirmChangedOnDisk' }));
      expect(log).toEqual(['prepare overwrite']);
      expect(asked).toEqual([]);
      expect(calls).toEqual([]);
      expect(result.current.dirty).toBe(true);
    });

    it('"overwrite" prepares an acknowledged overwrite, then saves', async () => {
      const log: Log = [];
      const { engine, asked } = savingEngine(log);
      const { prepared, calls, delivery } = changedDelivery(log);
      const { result } = renderHook(() => useSave(engine, document, delivery));
      act(() => result.current.save());
      await waitFor(() => expect(result.current.activity.kind).toBe('confirmChangedOnDisk'));

      act(() => result.current.overwriteChanged());
      await waitFor(() => expect(result.current.activity.kind).toBe('done'));
      expect(prepared.map((p) => p.intent)).toEqual(['overwrite', 'overwriteAnyway']);
      expect(asked).toEqual([[7, false]]);
      expect(calls).toEqual([{ bytes, name: 'a.pdf', destination: overwrite }]);
    });

    it('"save a copy" prepares a copy and saves it, without touching the file', async () => {
      const log: Log = [];
      const { engine, asked } = savingEngine(log);
      const { prepared, calls, delivery } = changedDelivery(log);
      const { result } = renderHook(() => useSave(engine, document, delivery));
      act(() => result.current.save());
      await waitFor(() => expect(result.current.activity.kind).toBe('confirmChangedOnDisk'));

      act(() => result.current.saveChangedCopy());
      await waitFor(() => expect(result.current.activity.kind).toBe('done'));
      expect(prepared.map((p) => p.intent)).toEqual(['overwrite', 'copy']);
      expect(asked).toEqual([[7, false]]);
      expect(calls).toEqual([{ bytes, name: 'a.pdf', destination: copy }]);
    });

    it('cancelling saves nothing and goes back to idle', async () => {
      const { engine, asked } = savingEngine();
      const { delivery } = changedDelivery([]);
      const { result } = renderHook(() => useSave(engine, document, delivery));
      act(() => result.current.save());
      await waitFor(() => expect(result.current.activity.kind).toBe('confirmChangedOnDisk'));

      act(() => result.current.cancelChanged());
      expect(result.current.activity).toEqual({ kind: 'idle' });
      expect(asked).toEqual([]);
    });
  });

  it('a signed document whose file changed: Overwrite then Save anyway writes once, carrying the accepted stamp', async () => {
    const accepted = { lastModified: 2000, size: 50 };
    const placement: Destination = { kind: 'overwrite', handle: undefined, permission: 'granted', seen: accepted };
    const stamps: unknown[] = [];
    const { calls, delivery: base } = deliveries([]);
    const delivery: Delivery = {
      ...base,
      prepare: async (intent, source, acceptedStamp) => {
        stamps.push(acceptedStamp);
        await base.prepare(intent, source);
        if (intent === 'overwriteAnyway') return placement;
        return acceptedStamp ? placement : { kind: 'changedOnDisk' };
      },
    };
    const { engine } = savingEngine([], signed);
    const { result } = renderHook(() => useSave(engine, document, delivery));
    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity.kind).toBe('confirmChangedOnDisk'));
    act(() => result.current.overwriteChanged());
    await waitFor(() => expect(result.current.activity.kind).toBe('confirmSignatures'));
    act(() => result.current.confirmSignatures());
    await waitFor(() => expect(result.current.activity.kind).toBe('done'));
    expect(stamps).toEqual([undefined, undefined, accepted]);
    expect(calls).toHaveLength(1);
  });

  it('a write that finds the file changed after the engine ran asks again and keeps the document dirty', async () => {
    const { engine } = savingEngine();
    const { delivery } = deliveries([], undefined, { how: 'changedOnDisk' });
    const { result } = renderHook(() => useSave(engine, document, delivery));
    act(() => result.current.markDirty());
    act(() => result.current.save());
    await waitFor(() => expect(result.current.activity).toEqual({ kind: 'confirmChangedOnDisk' }));
    expect(result.current.dirty).toBe(true);
  });

  it('a signature question raised by a copy is marked as one, so it cannot offer to overwrite', async () => {
    const { engine } = savingEngine([], signed);
    const { delivery } = deliveries([], { kind: 'copy' });
    const { result } = renderHook(() => useSave(engine, document, delivery));
    act(() => result.current.saveChangedCopy());
    await waitFor(() => expect(result.current.activity).toEqual({ kind: 'confirmSignatures', forCopy: true }));
  });
});
