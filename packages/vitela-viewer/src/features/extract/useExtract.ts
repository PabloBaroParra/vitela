// Extract pages: which pages, then a new PDF of them — the order of the
// native shells. Both gates (the document withholds extracting, or its
// encryption cannot survive the rewrite) are the core's and are asked as the
// dialog opens, so it can say why before anything runs; the page grammar is
// the core's too (`parsePageSelection`), and what it refuses is shown with
// the dialog still open.
//
// The new PDF is cut from the session as it is now, after the queued edits
// (as Save waits for them), so unsaved page edits, annotations and fills are
// in it. It is protected when the source is (the core's rule), and from a
// signed source its signature no longer verifies, which the status says. It
// is delivered as a new file, like a compressed copy. Nothing about the open
// document changes. Container logic only, no markup.

import { useCallback, useEffect, useRef, useState } from 'react';
import { INVALID_PAGE_SELECTION, errorKind, errorMessage, type DocumentHandle, type Engine } from '@vitela/core';
import type { OpenDocument } from '../document/useDocument';
import type { Delivery } from '../save/delivery';
import { sentence } from '../../ui/text';
import { EXTRACT_RANGE_MISSING, extractStatus, extractedName } from './extractText';

export type ExtractActivity =
  | { readonly kind: 'idle' }
  /** Asking the gates before the dialog opens. */
  | { readonly kind: 'checking' }
  /** The dialog is open. */
  | {
      readonly kind: 'choosing';
      /** What the field starts with: an Organize selection, or nothing. */
      readonly initialRange: string;
      /** Why no page can be extracted, as a sentence; `null` when they can. */
      readonly refusal: string | null;
      /** The file as opened is signed: the summary says its copy's signature breaks. */
      readonly signed: boolean;
      /** The refusal of the last range sent, as a sentence. */
      readonly error: string | null;
      /** The range is with the core: the dialog waits. */
      readonly running: boolean;
    }
  /** The new PDF is on its way to the delivery. */
  | { readonly kind: 'delivering' }
  | { readonly kind: 'done'; readonly message: string }
  | { readonly kind: 'failed'; readonly message: string };

const IDLE: ExtractActivity = { kind: 'idle' };

/** From the click until the PDF is delivered: Extract cannot be started again. */
export function extractBusy(activity: ExtractActivity): boolean {
  return activity.kind === 'checking' || activity.kind === 'choosing' || activity.kind === 'delivering';
}

/** What the dialog says before anything is typed. */
export interface ExtractGates {
  readonly refusal: string | null;
  readonly signed: boolean;
}

/**
 * The core's two questions before Extract or Split is offered. A failed
 * question refuses rather than guessing: a guess could offer an action the
 * core then refuses, or leave out the signature note.
 */
export async function askExtractGates(engine: Engine, handle: DocumentHandle): Promise<ExtractGates> {
  try {
    const refused = await engine.extractionRefusal(handle);
    if (refused) return { refusal: sentence(refused), signed: false };
    return { refusal: null, signed: await engine.extractSourceIsSigned(handle) };
  } catch (error) {
    return { refusal: sentence(errorMessage(error)), signed: false };
  }
}

export type ExtractSession = ReturnType<typeof useExtract>;

export function useExtract(
  engine: Engine,
  document: OpenDocument | null,
  delivery: Delivery,
  /** Settles once the edits the editing screen has asked for so far have ended. */
  settleEdits?: () => Promise<unknown>,
  /** The page count once those edits have landed (a render may not have brought it yet). */
  pageCountNow?: () => number,
) {
  const generation = document?.generation;
  const [activity, setActivity] = useState<ExtractActivity>(IDLE);
  const [forGeneration, setForGeneration] = useState(generation);
  /** The document on screen: an answer for an old document changes nothing. */
  const shown = useRef(generation);
  /** True from the click until the dialog closes: one at a time. */
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

  /** Opens the dialog, its field holding `initialRange` (an Organize selection as `"2-3"`). */
  const open = useCallback(
    async (initialRange = '') => {
      if (!document || busy.current) return;
      busy.current = true;
      const asking = document.generation;
      setActivity({ kind: 'checking' });
      const { refusal, signed } = await askExtractGates(engine, document.handle);
      if (shown.current !== asking) return;
      setActivity({ kind: 'choosing', initialRange, refusal, signed, error: null, running: false });
    },
    [engine, document],
  );

  const cancel = useCallback(() => {
    setActivity((a) => (a.kind === 'choosing' && !a.running ? IDLE : a));
    busy.current = false;
  }, []);

  const extract = useCallback(
    async (range: string) => {
      if (!document || activity.kind !== 'choosing' || activity.refusal !== null || activity.running) return;
      if (range.trim() === '') {
        setActivity({ ...activity, error: EXTRACT_RANGE_MISSING });
        return;
      }
      const running = document.generation;
      const current = () => shown.current === running;
      setActivity({ ...activity, error: null, running: true });
      let pages: number[];
      let bytes: Uint8Array;
      try {
        // The edits asked for before the click go in.
        await settleEdits?.();
        pages = await engine.parsePageSelection(range, pageCountNow?.() ?? document.info.pageCount);
        bytes = await engine.extractPages(document.handle, pages);
      } catch (error) {
        if (!current()) return;
        if (errorKind(error) === INVALID_PAGE_SELECTION) {
          // The core's sentence: the dialog stays, so the range can be corrected.
          setActivity({ ...activity, error: sentence(errorMessage(error)), running: false });
        } else {
          setActivity({ kind: 'failed', message: errorMessage(error) });
          busy.current = false;
        }
        return;
      }
      if (!current()) return;
      setActivity({ kind: 'delivering' });
      const { signed } = activity;
      try {
        const outcome = await delivery.deliver({ bytes, name: extractedName(document.name), destination: { kind: 'newFile' } });
        if (!current()) return;
        // A new file is never compared with the disk, so this cannot happen; say nothing was written if it does.
        const said = outcome.how === 'changedOnDisk' ? ({ how: 'cancelled' } as const) : outcome;
        setActivity({ kind: 'done', message: extractStatus(said, pages.length, signed) });
      } catch (error) {
        if (current()) setActivity({ kind: 'failed', message: errorMessage(error) });
      } finally {
        if (current()) busy.current = false;
      }
    },
    [engine, document, delivery, settleEdits, pageCountNow, activity],
  );

  return { activity, open, cancel, extract };
}
