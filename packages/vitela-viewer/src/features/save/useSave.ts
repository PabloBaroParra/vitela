// Saving the open document: asks the user for what a save needs while the
// click is fresh, asks the engine for the bytes, asks again before a save that
// would break a signature, and delivers the bytes (over the opened file, as a
// new copy, or as a download). Container logic only, no markup.
//
// `dirty` is the "unsaved edits" flag. Nothing edits a document yet (Organize
// is T-226): the editing screens call `markDirty()` after each applied edit.
// Only a save that WROTE over the opened file clears it, and only if no edit
// came in while it ran: a download or a copy leaves the opened file without
// the edits. The state belongs to one document: opening another starts clean.
//
// An editing screen queues its edits and sends each to the engine only once
// the ones before it have ended, so a save waits for the edits asked for so
// far (`waitForEdits`) before it asks for the bytes: a fill the user made
// just before clicking Save is in the file.

import { useCallback, useEffect, useRef, useState } from 'react';
import { SIGNATURES_WOULD_BE_INVALIDATED, errorKind, errorMessage } from '@vitela/core';
import type { FileStamp } from '../../platform/fileStamp';
import type { Engine } from '@vitela/core';
import type { OpenDocument } from '../document/useDocument';
import { browserDelivery, type Delivery, type SaveIntent } from './delivery';
import type { SaveOutcome } from './outcome';

export type SaveActivity =
  | { readonly kind: 'idle' }
  | { readonly kind: 'saving' }
  /** The save would break a signature; waiting for the user's answer. */
  | { readonly kind: 'confirmSignatures'; readonly forCopy: boolean }
  /** The opened file changed on disk; waiting for the user's answer. */
  | { readonly kind: 'confirmChangedOnDisk' }
  | { readonly kind: 'done'; readonly outcome: SaveOutcome }
  | { readonly kind: 'failed'; readonly message: string };

const IDLE: SaveActivity = { kind: 'idle' };

export type SaveSession = ReturnType<typeof useSave>;

export function useSave(engine: Engine, document: OpenDocument | null, delivery: Delivery = browserDelivery) {
  const generation = document?.generation;
  const [activity, setActivity] = useState<SaveActivity>(IDLE);
  const [dirty, setDirty] = useState(false);
  const [forGeneration, setForGeneration] = useState(generation);
  /** The document whose save is in flight: a save for an old document never blocks the new one. */
  const running = useRef<number | undefined>(undefined);
  /** Counts edits, so a save does not clear the mark of an edit made while it ran. */
  const edits = useRef(0);
  /** The document on screen: a save that outlives its document reports nothing. */
  const shown = useRef(generation);
  /** The on-disk change the user accepted in the save being retried (e.g. after a signature question). */
  const accepted = useRef<FileStamp | undefined>(undefined);
  /** Settles once every edit the editing screen has asked for so far has ended. */
  const editsSettled = useRef<(() => Promise<unknown>) | null>(null);

  // A new document starts with a clean slate (adjusting state while rendering
  // is React's way to reset state when an input changes).
  if (forGeneration !== generation) {
    setForGeneration(generation);
    setActivity(IDLE);
    setDirty(false);
  }
  useEffect(() => {
    shown.current = generation;
    edits.current = 0;
  }, [generation]);

  const run = useCallback(
    async (intent: SaveIntent, invalidateSignatures: boolean) => {
      if (!document || running.current === document.generation) return;
      const saving = document.generation;
      running.current = saving;
      const current = () => shown.current === saving;
      setActivity({ kind: 'saving' });
      try {
        // First, while the click is fresh: the permission prompt or save dialog.
        const destination = await delivery.prepare(intent, document, accepted.current);
        if (!destination) {
          if (current()) setActivity({ kind: 'done', outcome: { how: 'cancelled' } });
          return;
        }
        if (intent === 'overwriteAnyway' && destination.kind === 'overwrite') accepted.current = destination.seen;
        if (destination.kind === 'changedOnDisk') {
          accepted.current = undefined;
          if (current()) setActivity({ kind: 'confirmChangedOnDisk' });
          return;
        }
        // The edits asked for before the click go in, and count as saved.
        await editsSettled.current?.();
        const editsAtStart = edits.current;
        const bytes = await engine.saveToBytes(document.handle, invalidateSignatures);
        // The document was closed while the engine worked: deliver nothing.
        if (!current()) return;
        const outcome = await delivery.deliver({ bytes, name: document.name, destination });
        if (!current()) return;
        // The file moved while the engine worked: nothing was written, so ask again.
        if (outcome.how === 'changedOnDisk') {
          setActivity({ kind: 'confirmChangedOnDisk' });
          return;
        }
        // Only writing over the opened file (or handing the bytes to the host
        // page) puts the edits where the document lives, and not an edit made since.
        accepted.current = undefined;
        const kept = outcome.how === 'wrote' || outcome.how === 'delivered';
        if (kept && edits.current === editsAtStart) setDirty(false);
        setActivity({ kind: 'done', outcome });
      } catch (error) {
        if (errorKind(error) !== SIGNATURES_WOULD_BE_INVALIDATED) accepted.current = undefined;
        if (!current()) return;
        setActivity(
          errorKind(error) === SIGNATURES_WOULD_BE_INVALIDATED
            ? { kind: 'confirmSignatures', forCopy: intent === 'copy' }
            : { kind: 'failed', message: errorMessage(error) },
        );
      } finally {
        if (running.current === saving) running.current = undefined;
      }
    },
    [engine, document, delivery],
  );

  const save = useCallback(() => ((accepted.current = undefined), void run('overwrite', false)), [run]);
  /** "Save anyway": over the opened file, breaking its signature. */
  const confirmSignatures = useCallback(() => void run('overwrite', true), [run]);
  /** "Save a copy": the acknowledged bytes go to a new file, the opened one stays as it is. */
  const saveSignedCopy = useCallback(() => void run('copy', true), [run]);
  const cancelSignatures = useCallback(() => (setActivity(IDLE), (accepted.current = undefined)), []);
  /** "Overwrite": the user saw the file changed on disk and wants it replaced. */
  const overwriteChanged = useCallback(() => void run('overwriteAnyway', false), [run]);
  /** "Save a copy": a download; the changed file is left as it is. */
  const saveChangedCopy = useCallback(() => void run('copy', false), [run]);
  const cancelChanged = useCallback(() => setActivity(IDLE), []);
  const markDirty = useCallback(() => {
    edits.current++;
    setDirty(true);
    // "Saved over x" no longer describes the document.
    setActivity((a) => (a.kind === 'done' ? IDLE : a));
  }, []);

  /** An editing screen hands over its queue; the result takes it back (when the screen goes). */
  const waitForEdits = useCallback((settled: () => Promise<unknown>) => {
    editsSettled.current = settled;
    return () => {
      if (editsSettled.current === settled) editsSettled.current = null;
    };
  }, []);

  /** Settles once the edits asked for so far have ended: what a compressed copy waits for too. */
  const settleEdits = useCallback(() => editsSettled.current?.() ?? Promise.resolve(), []);

  return {
    activity,
    dirty,
    settleEdits,
    save,
    confirmSignatures,
    saveSignedCopy,
    cancelSignatures,
    overwriteChanged,
    saveChangedCopy,
    cancelChanged,
    markDirty,
    waitForEdits,
  };
}
