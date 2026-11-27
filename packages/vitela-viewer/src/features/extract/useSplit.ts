// Split: where to cut, then one PDF per part — the order of the native
// shells. The gates are Extract's (each part is an extraction), asked as the
// dialog opens; a one-page document is refused there too, in the core's
// words. The cut grammar, the parts and their names are the core's
// (`planSplit`: "2,4" of five pages is report-part1..3.pdf on every shell),
// and what it refuses is shown with the dialog still open.
//
// The parts are cut from the session as it is, after the queued edits. The
// engine is asked for at most one part ahead of the one being written, like
// the pages of an image export: a cancel stops the work as well as the
// downloads, and no more than two parts' bytes are held at once. So an edit
// made while the split runs reaches the parts asked for after it lands (the
// worker answers requests in order). Each part goes out as its own download
// the moment it is ready: the split stops at the first part that fails, when
// the user cancels (after the part in hand), or when the editor goes away,
// and what was written is counted. The native shells ask for a folder and one
// overwrite prompt; a browser download overwrites nothing, so there is no
// prompt. Nothing about the open document changes. Container logic only, no
// markup.

import { useCallback, useEffect, useRef, useState } from 'react';
import { INVALID_PAGE_SELECTION, errorKind, errorMessage, type Engine, type SplitPart } from '@vitela/core';
import type { OpenDocument } from '../document/useDocument';
import type { Delivery } from '../save/delivery';
import { sentence } from '../../ui/text';
import { splitCancelled, splitDone, splitPartFailed, splitWriteFailed } from './extractText';
import { askExtractGates } from './useExtract';

export type SplitActivity =
  | { readonly kind: 'idle' }
  /** Asking the gates before the dialog opens. */
  | { readonly kind: 'checking' }
  /** The dialog is open. */
  | {
      readonly kind: 'choosing';
      /** Why the document cannot be split, as a sentence; `null` when it can. */
      readonly refusal: string | null;
      readonly signed: boolean;
      /** The refusal of the last cuts sent, as a sentence. */
      readonly error: string | null;
      /** The cuts are with the core: the dialog waits. */
      readonly running: boolean;
    }
  /** `index` counts the parts from 1. */
  | { readonly kind: 'splitting'; readonly index: number; readonly total: number }
  | { readonly kind: 'done'; readonly message: string }
  | { readonly kind: 'failed'; readonly message: string };

const IDLE: SplitActivity = { kind: 'idle' };

/** From the click until the dialog closes or the split ends: Split cannot be started again. */
export function splitBusy(activity: SplitActivity): boolean {
  return activity.kind === 'checking' || activity.kind === 'choosing' || activity.kind === 'splitting';
}

const partPages = ({ first, last }: SplitPart) => Array.from({ length: last - first + 1 }, (_, i) => first + i);

export type SplitSession = ReturnType<typeof useSplit>;

export function useSplit(
  engine: Engine,
  document: OpenDocument | null,
  delivery: Delivery,
  /** Settles once the edits the editing screen has asked for so far have ended. */
  settleEdits?: () => Promise<unknown>,
  /** The page count once those edits have landed (a render may not have brought it yet). */
  pageCountNow?: () => number,
) {
  const generation = document?.generation;
  const [activity, setActivity] = useState<SplitActivity>(IDLE);
  const [forGeneration, setForGeneration] = useState(generation);
  /** The document on screen: an answer for an old document changes nothing. */
  const shown = useRef(generation);
  /** False once the editor is gone: a running split stops at its next part. */
  const mounted = useRef(true);
  /** Set by Cancel split: the running split stops after the part in hand. */
  const stopRequested = useRef(false);
  /** True from the click until the dialog closes or the split ends: one at a time. */
  const busy = useRef(false);

  // A new document starts clean (adjusting state while rendering).
  if (forGeneration !== generation) {
    setForGeneration(generation);
    setActivity(IDLE);
  }
  useEffect(() => {
    shown.current = generation;
    busy.current = false;
  }, [generation]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const open = useCallback(async () => {
    if (!document || busy.current) return;
    busy.current = true;
    const asking = document.generation;
    setActivity({ kind: 'checking' });
    let { refusal, signed } = await askExtractGates(engine, document.handle);
    if (refusal === null && document.info.pageCount < 2) {
      // The core's own sentence for a document with nothing to cut, said up
      // front as the Windows shell's preflight plan says it.
      try {
        await engine.planSplit('1', document.info.pageCount, document.name);
      } catch (error) {
        refusal = sentence(errorMessage(error));
        signed = false;
      }
    }
    if (shown.current !== asking) return;
    setActivity({ kind: 'choosing', refusal, signed, error: null, running: false });
  }, [engine, document]);

  const cancel = useCallback(() => {
    setActivity((a) => (a.kind === 'choosing' && !a.running ? IDLE : a));
    busy.current = false;
  }, []);

  /** Cancel split: the part in hand still goes out, then the split stops. */
  const stop = useCallback(() => {
    stopRequested.current = true;
  }, []);

  /** Delivers the parts as they are made; the plan has already been checked. */
  const run = useCallback(
    async (parts: readonly SplitPart[], make: (part: SplitPart) => Promise<Uint8Array>, signed: boolean, current: () => boolean) => {
      let written = 0;
      /** The part in hand, already asked for; the next one is asked for while it is written. */
      let making: Promise<Uint8Array> | null = parts[0] ? make(parts[0]) : null;
      for (const [i, part] of parts.entries()) {
        if (!current()) return;
        if (stopRequested.current) {
          setActivity({ kind: 'done', message: splitCancelled(written) });
          return;
        }
        setActivity({ kind: 'splitting', index: i + 1, total: parts.length });
        let bytes: Uint8Array;
        try {
          bytes = await making!;
        } catch (error) {
          if (current()) setActivity({ kind: 'failed', message: splitPartFailed(part.fileName, errorMessage(error), written) });
          return;
        }
        if (!current()) return;
        const following = parts[i + 1];
        making = following && !stopRequested.current ? make(following) : null;
        try {
          await delivery.deliver({ bytes, name: part.fileName, destination: { kind: 'newFile' } });
        } catch {
          if (current()) setActivity({ kind: 'failed', message: splitWriteFailed(part.fileName, written) });
          return;
        }
        written++;
      }
      if (!current()) return;
      // A cancel pressed during the last part came too late to save anything: it all went out.
      setActivity({ kind: 'done', message: splitDone(written, signed) });
    },
    [delivery],
  );

  const split = useCallback(
    async (cuts: string) => {
      if (!document || activity.kind !== 'choosing' || activity.refusal !== null || activity.running) return;
      const running = document.generation;
      const current = () => mounted.current && shown.current === running;
      stopRequested.current = false;
      setActivity({ ...activity, error: null, running: true });
      let parts: SplitPart[];
      try {
        // The edits asked for before the click go in.
        await settleEdits?.();
        parts = await engine.planSplit(cuts, pageCountNow?.() ?? document.info.pageCount, document.name);
      } catch (error) {
        if (!current()) return;
        if (errorKind(error) === INVALID_PAGE_SELECTION) {
          // The core's sentence: the dialog stays, so the cuts can be corrected.
          setActivity({ ...activity, error: sentence(errorMessage(error)), running: false });
        } else {
          setActivity({ kind: 'failed', message: errorMessage(error) });
          busy.current = false;
        }
        return;
      }
      if (!current()) return;
      const make = (part: SplitPart) => {
        const made = engine.extractPages(document.handle, partPages(part));
        // A part the split never reaches (cancelled, failed earlier) must not reject unheard.
        made.catch(() => undefined);
        return made;
      };
      try {
        await run(parts, make, activity.signed, current);
      } finally {
        if (current()) busy.current = false;
      }
    },
    [engine, document, settleEdits, pageCountNow, activity, run],
  );

  return { activity, open, cancel, stop, split };
}
