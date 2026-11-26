// Document properties: the six text keys of `/Info` in a dialog, applied as
// one undoable edit — Android's Document properties (Windows and Linux show
// the same keys in a panel). The values shown are what the next save would
// write (the engine reads the last pending change, else the file's), and the
// gate is asked as the dialog opens: a document whose permissions withhold
// modification shows its properties read-only, with the core's sentence.
//
// Apply waits for the edits asked for before it, reads the properties again
// (an undo may have landed since the dialog opened, as Android re-reads under
// its lane) and records only a real change: the boxes saying what the
// document has close the dialog with nothing recorded. A change is a step in
// the same history as every other edit, so Undo takes it back; it marks the
// document dirty and is written by Save. The apply is handed to the save, so
// a save asked for meanwhile waits for it. A refusal keeps the dialog open
// with what was typed. Container logic only, no markup.

import { useCallback, useEffect, useRef, useState } from 'react';
import { errorMessage, type DocumentHandle, type EditedDocument, type Engine } from '@vitela/core';
import type { OpenDocument } from '../document/useDocument';
import { sentence } from '../../ui/text';
import { PROPERTIES_UPDATED, draftOf, propertiesChange, type PropertiesDraft } from './propertiesText';

export type PropertiesActivity =
  | { readonly kind: 'idle' }
  /** Reading the properties and the gate before the dialog opens. */
  | { readonly kind: 'checking' }
  /** The dialog is open. */
  | {
      readonly kind: 'editing';
      readonly draft: PropertiesDraft;
      /** Why the properties are read-only, as a sentence; `null` when they can be changed. */
      readonly refusal: string | null;
      /** Why the last apply did not go in, as a sentence. */
      readonly error: string | null;
      /** The change is with the core: the dialog waits. */
      readonly running: boolean;
    }
  | { readonly kind: 'done'; readonly message: string }
  | { readonly kind: 'failed'; readonly message: string };

const IDLE: PropertiesActivity = { kind: 'idle' };

/** From the click until the dialog closes: Properties cannot be opened again. */
export function propertiesBusy(activity: PropertiesActivity): boolean {
  return activity.kind === 'checking' || activity.kind === 'editing';
}

/** What the workspace lends the dialog: the edit queue, the history and the save state. */
export interface PropertiesHost {
  /** Settles once the edits the editing screen has asked for so far have ended. */
  readonly settleEdits: () => Promise<unknown>;
  /** Hands the dialog's own apply to the save (see `useSave`); the result takes it back. */
  readonly waitForEdits: (settled: () => Promise<unknown>) => () => void;
  /** Reports the edited document; `false` when it is no longer the one on screen. */
  readonly onEdited: (handle: DocumentHandle, edited: EditedDocument) => boolean;
  readonly markDirty: () => void;
}

export type PropertiesSession = ReturnType<typeof useDocumentProperties>;

export function useDocumentProperties(
  engine: Engine,
  document: OpenDocument | null,
  { settleEdits, waitForEdits, onEdited, markDirty }: PropertiesHost,
) {
  const generation = document?.generation;
  const [activity, setActivity] = useState<PropertiesActivity>(IDLE);
  const [forGeneration, setForGeneration] = useState(generation);
  /** The document on screen: an answer for an old document changes nothing. */
  const shown = useRef(generation);
  const mounted = useRef(true);
  /** True from the click until the dialog closes: one at a time. */
  const busy = useRef(false);
  /** Settles once the apply in flight, if any, has ended. */
  const applying = useRef<Promise<unknown>>(Promise.resolve());

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
  const applied = useCallback(() => applying.current, []);
  useEffect(() => waitForEdits(applied), [waitForEdits, applied]);

  const open = useCallback(async () => {
    if (!document || busy.current) return;
    busy.current = true;
    const asking = document.generation;
    const current = () => mounted.current && shown.current === asking;
    setActivity({ kind: 'checking' });
    try {
      const properties = await engine.documentProperties(document.handle);
      const refusal = await engine.metadataRefusal(document.handle);
      if (!current()) return;
      setActivity({
        kind: 'editing',
        draft: draftOf(properties),
        refusal: refusal === null ? null : sentence(refusal),
        error: null,
        running: false,
      });
    } catch (error) {
      if (!current()) return;
      setActivity({ kind: 'failed', message: `Could not read the document properties: ${errorMessage(error)}` });
      busy.current = false;
    }
  }, [engine, document]);

  /** A changed box clears the last refusal: it was about the old values. */
  const edit = useCallback((draft: PropertiesDraft) => {
    setActivity((a) => (a.kind === 'editing' && a.refusal === null && !a.running ? { ...a, draft, error: null } : a));
  }, []);

  const cancel = useCallback(() => {
    setActivity((a) => (a.kind === 'editing' && !a.running ? IDLE : a));
    busy.current = false;
  }, []);

  const apply = useCallback(async () => {
    if (!document || activity.kind !== 'editing' || activity.refusal !== null || activity.running) return;
    const { draft } = activity;
    const { handle } = document;
    const running = document.generation;
    const current = () => mounted.current && shown.current === running;
    setActivity({ ...activity, error: null, running: true });
    const step = (async () => {
      try {
        // The edits asked for before the click go in first.
        await settleEdits();
        const change = propertiesChange(await engine.documentProperties(handle), draft);
        if (change === null) {
          // Nothing to record: the dialog closes and Properties can open again.
          if (current()) {
            setActivity(IDLE);
            busy.current = false;
          }
          return;
        }
        const edited = await engine.applyEdit(handle, { kind: 'setDocumentProperties', properties: change });
        // Reported even if the editor went away: the core has the edit either way.
        if (onEdited(handle, edited)) markDirty();
        if (current()) setActivity({ kind: 'done', message: PROPERTIES_UPDATED });
      } catch (error) {
        // Only the message changes: the dialog stays open on what was typed.
        if (current()) setActivity({ kind: 'editing', draft, refusal: null, error: sentence(errorMessage(error)), running: false });
        return;
      }
      if (current()) busy.current = false;
    })();
    applying.current = step;
    await step;
  }, [engine, document, activity, settleEdits, onEdited, markDirty]);

  return { activity, open, edit, cancel, apply };
}
