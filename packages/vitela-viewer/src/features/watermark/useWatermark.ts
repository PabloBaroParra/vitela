// Add watermark: the same text or picture stamped on the pages the reader
// picks, as one undoable edit — the native shells' Add watermark (Android's
// dialog, Windows' and Linux's tile). The starting look and the gate are the
// core's, asked as the dialog opens: a document that withholds modification
// (or whose encryption cannot survive the rewrite a stamp forces) gets the
// core's sentence and only Close.
//
// Add waits for the edits asked for before it, resolves the pages (all, the
// one the viewer last showed, or a typed range read by the core's grammar,
// against the page count the last edit left), and records one stamp. It is
// a step in the same history as every other edit, so Undo takes it back; it
// marks the document dirty and is written by Save, which waits for it. A
// refusal (a character Helvetica cannot show, a picture that does not
// decode, a bad range) keeps the dialog open with the draft. Container
// logic only, no markup.

import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ENCODING_GAP,
  INVALID_IMAGE,
  errorKind,
  errorMessage,
  type DocumentHandle,
  type EditedDocument,
  type Engine,
  type WatermarkDefaults,
} from '@vitela/core';
import { MAX_STAMP_BYTES } from '../annotations/stamps';
import type { OpenDocument } from '../document/useDocument';
import { sentence } from '../../ui/text';
import {
  FONT_GAP_HINT,
  NO_PAGES,
  PICTURE_TOO_LARGE,
  PICTURE_UNREADABLE,
  RANGE_MISSING,
  draftOf,
  markRefusal,
  switchKind,
  watermarkEdit,
  watermarkStatus,
  type WatermarkDraft,
  type WatermarkKind,
} from './watermarkText';

export type WatermarkActivity =
  | { readonly kind: 'idle' }
  /** Reading the starting look and the gate before the dialog opens. */
  | { readonly kind: 'checking' }
  /** The dialog is open. */
  | {
      readonly kind: 'editing';
      readonly draft: WatermarkDraft;
      readonly defaults: WatermarkDefaults;
      /** Why no stamp can be added, as a sentence; `null` when one can. */
      readonly refusal: string | null;
      /** Why the last Add did not go in, as a sentence. */
      readonly error: string | null;
      /** The stamp is with the core: the dialog waits. */
      readonly running: boolean;
    }
  | { readonly kind: 'done'; readonly message: string }
  | { readonly kind: 'failed'; readonly message: string };

const IDLE: WatermarkActivity = { kind: 'idle' };

/** From the click until the dialog closes: Add watermark cannot be opened again. */
export function watermarkBusy(activity: WatermarkActivity): boolean {
  return activity.kind === 'checking' || activity.kind === 'editing';
}

/** What the workspace lends the dialog: the edit queue, the history, the save state and the pages. */
export interface WatermarkHost {
  /** Settles once the edits the editing screen has asked for so far have ended. */
  readonly settleEdits: () => Promise<unknown>;
  /** Hands the dialog's own Add to the save (see `useSave`); the result takes it back. */
  readonly waitForEdits: (settled: () => Promise<unknown>) => () => void;
  /** Reports the edited document; `false` when it is no longer the one on screen. */
  readonly onEdited: (handle: DocumentHandle, edited: EditedDocument) => boolean;
  readonly markDirty: () => void;
  /** The page the viewer last showed: "Current page". */
  readonly currentPage: () => number;
  /** The page count after the last edit applied, not the one at the click. */
  readonly pageCountNow: () => number;
}

export type WatermarkSession = ReturnType<typeof useWatermark>;

/** A refusal of the core's, in the dialog's words. */
function refusalOf(error: unknown): string {
  const kind = errorKind(error);
  if (kind === INVALID_IMAGE) return PICTURE_UNREADABLE;
  const message = sentence(errorMessage(error));
  return kind === ENCODING_GAP ? `${message} ${FONT_GAP_HINT}` : message;
}

export function useWatermark(
  engine: Engine,
  document: OpenDocument | null,
  { settleEdits, waitForEdits, onEdited, markDirty, currentPage, pageCountNow }: WatermarkHost,
) {
  const generation = document?.generation;
  const [activity, setActivity] = useState<WatermarkActivity>(IDLE);
  const [forGeneration, setForGeneration] = useState(generation);
  /** The document on screen: an answer for an old document changes nothing. */
  const shown = useRef(generation);
  const mounted = useRef(true);
  /** True from the click until the dialog closes: one at a time. */
  const busy = useRef(false);
  /** Settles once the stamp in flight, if any, has ended. */
  const adding = useRef<Promise<unknown>>(Promise.resolve());

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
  const added = useCallback(() => adding.current, []);
  useEffect(() => waitForEdits(added), [waitForEdits, added]);

  const open = useCallback(async () => {
    if (!document || busy.current) return;
    busy.current = true;
    const asking = document.generation;
    const current = () => mounted.current && shown.current === asking;
    setActivity({ kind: 'checking' });
    try {
      const defaults = await engine.watermarkDefaults();
      const refusal = await engine.watermarkRefusal(document.handle);
      if (!current()) return;
      setActivity({
        kind: 'editing',
        draft: draftOf(defaults),
        defaults,
        refusal: refusal === null ? null : sentence(refusal),
        error: null,
        running: false,
      });
    } catch (error) {
      if (!current()) return;
      setActivity({ kind: 'failed', message: `Could not open Add watermark: ${errorMessage(error)}` });
      busy.current = false;
    }
  }, [engine, document]);

  /** A changed choice clears the last refusal: it was about the old draft. */
  const edit = useCallback((draft: WatermarkDraft) => {
    setActivity((a) => (a.kind === 'editing' && a.refusal === null && !a.running ? { ...a, draft, error: null } : a));
  }, []);

  const switchTo = useCallback((kind: WatermarkKind) => {
    setActivity((a) =>
      a.kind === 'editing' && a.refusal === null && !a.running
        ? { ...a, draft: switchKind(a.draft, kind, a.defaults), error: null }
        : a,
    );
  }, []);

  /**
   * The picked file's bytes become the draft's picture; whether they decode is
   * the core's call on Add. A file over the image-stamp cap is refused before
   * it is read (the draft keeps its earlier picture), as the Image tool does.
   */
  const pickPicture = useCallback(async (file: File) => {
    if (file.size > MAX_STAMP_BYTES) {
      setActivity((a) => (a.kind === 'editing' && !a.running ? { ...a, error: PICTURE_TOO_LARGE } : a));
      return;
    }
    let bytes: Uint8Array;
    try {
      bytes = new Uint8Array(await file.arrayBuffer());
    } catch {
      setActivity((a) => (a.kind === 'editing' && !a.running ? { ...a, error: PICTURE_UNREADABLE } : a));
      return;
    }
    const picture = { name: file.name, bytes };
    setActivity((a) =>
      a.kind === 'editing' && a.refusal === null && !a.running ? { ...a, draft: { ...a.draft, picture }, error: null } : a,
    );
  }, []);

  const cancel = useCallback(() => {
    setActivity((a) => (a.kind === 'editing' && !a.running ? IDLE : a));
    busy.current = false;
  }, []);

  const add = useCallback(async () => {
    if (!document || activity.kind !== 'editing' || activity.refusal !== null || activity.running) return;
    const { draft } = activity;
    const refused = markRefusal(draft) ?? (draft.pages === 'custom' && draft.customRange.trim() === '' ? RANGE_MISSING : null);
    if (refused !== null) {
      setActivity({ ...activity, error: refused });
      return;
    }
    const { handle } = document;
    const running = document.generation;
    const current = () => mounted.current && shown.current === running;
    const keep = activity;
    setActivity({ ...activity, error: null, running: true });
    const step = (async () => {
      try {
        // The edits asked for before the click go in first.
        await settleEdits();
        const total = pageCountNow();
        if (total === 0) throw new Error(NO_PAGES);
        const pages =
          draft.pages === 'all'
            ? Array.from({ length: total }, (_, index) => index)
            : draft.pages === 'current'
              ? // Clamped: the page shown can sit past the end right after a deletion.
                [Math.min(Math.max(currentPage(), 0), total - 1)]
              : await engine.parsePageSelection(draft.customRange, total);
        const edited = await engine.applyEdit(handle, watermarkEdit(draft, pages));
        // Reported even if the editor went away: the core has the edit either way.
        if (onEdited(handle, edited)) markDirty();
        if (current()) setActivity({ kind: 'done', message: watermarkStatus(pages.length) });
      } catch (error) {
        // Only the message changes: the dialog stays open on the draft.
        if (current()) setActivity({ ...keep, draft, error: refusalOf(error), running: false });
        return;
      }
      if (current()) busy.current = false;
    })();
    adding.current = step;
    await step;
  }, [engine, document, activity, settleEdits, onEdited, markDirty, currentPage, pageCountNow]);

  return { activity, open, edit, switchTo, pickPicture, cancel, add };
}
