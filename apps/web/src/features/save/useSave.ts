// Saving the open document: asks the engine for the bytes, asks the user
// before a save that would break a signature, and delivers the bytes (over
// the opened file or as a download). Container logic only, no markup.
//
// `dirty` is the "unsaved edits" flag. Nothing edits a document yet (Organize
// is T-226): the editing screens call `markDirty()` after each applied edit,
// and a successful save clears it.

import { useCallback, useRef, useState } from 'react';
import { SIGNATURES_WOULD_BE_INVALIDATED, errorKind, errorMessage } from '../../core/errors';
import type { Engine } from '../../core/types';
import type { OpenDocument } from '../document/useDocument';
import type { WritableFileHandle } from '../../platform/fileHandle';
import { saveBytes, type SaveOutcome } from './saveBytes';

export type SaveActivity =
  | { readonly kind: 'idle' }
  | { readonly kind: 'saving' }
  /** The save would break a signature; waiting for the user's answer. */
  | { readonly kind: 'confirmSignatures' }
  | { readonly kind: 'done'; readonly outcome: SaveOutcome }
  | { readonly kind: 'failed'; readonly message: string };

type Deliver = (request: {
  bytes: Uint8Array;
  name: string;
  handle: WritableFileHandle | undefined;
}) => Promise<SaveOutcome>;

const IDLE: SaveActivity = { kind: 'idle' };

export function useSave(engine: Engine, document: OpenDocument | null, deliver: Deliver = saveBytes) {
  const [activity, setActivity] = useState<SaveActivity>(IDLE);
  const [dirty, setDirty] = useState(false);
  const running = useRef(false);
  /** Counts edits, so a save does not clear the mark of an edit made while it ran. */
  const edits = useRef(0);

  const run = useCallback(
    async (invalidateSignatures: boolean) => {
      if (!document || running.current) return;
      running.current = true;
      const editsAtStart = edits.current;
      setActivity({ kind: 'saving' });
      try {
        const bytes = await engine.saveToBytes(document.handle, invalidateSignatures);
        const outcome = await deliver({ bytes, name: document.name, handle: document.fileHandle });
        if (edits.current === editsAtStart) setDirty(false);
        setActivity({ kind: 'done', outcome });
      } catch (error) {
        setActivity(
          errorKind(error) === SIGNATURES_WOULD_BE_INVALIDATED
            ? { kind: 'confirmSignatures' }
            : { kind: 'failed', message: errorMessage(error) },
        );
      } finally {
        running.current = false;
      }
    },
    [engine, document, deliver],
  );

  const save = useCallback(() => void run(false), [run]);
  const confirmSignatures = useCallback(() => void run(true), [run]);
  const cancelSignatures = useCallback(() => setActivity(IDLE), []);
  const markDirty = useCallback(() => {
    edits.current++;
    setDirty(true);
  }, []);

  return { activity, dirty, save, confirmSignatures, cancelSignatures, markDirty };
}
