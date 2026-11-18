// The viewer's edits: markup over a text selection, and undo/redo of the
// document's history (any step: an annotation here, or a page edit made in
// Organize). Sends them to the engine one at a time and reports every
// edited document. Container logic only.
//
// The rules are the core's: whether the document lets annotations be edited
// is known at open (`annotationEditingAllowed`) and is asked again on every
// edit (`EditNotPermitted`), as Organize does with page assembly. The engine
// refreshes its preview only when a step changes what pdfium draws, inside
// the same request.
//
// Steps run in the order they were asked for: an undo pressed while a
// three-line markup is still being added waits for its last line. A second
// markup while one runs is dropped (the selection it was made from is gone).
// The report (`onEdited`) is made even if the viewer went away meanwhile:
// the core has the edit either way.

import { useCallback, useRef, useState } from 'react';
import { EDIT_NOT_PERMITTED, errorKind, errorMessage } from '@vitela/core';
import type { EditedDocument, Engine, MarkupKind, PageRects, Rgb } from '@vitela/core';
import type { OpenDocument } from '../document/useDocument';
import { markupEdits } from './geometry';

/** The markup colour of the native shells (their `DEFAULT_COLOR`). */
export const MARKUP_COLOR: Rgb = { r: 255, g: 220, b: 0 };

export type AnnotationEditing = ReturnType<typeof useAnnotationEdits>;

export function useAnnotationEdits(engine: Engine, document: OpenDocument, onEdited: (edited: EditedDocument) => void) {
  const [running, setRunning] = useState(0);
  const [marking, setMarking] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [denied, setDenied] = useState(false);
  /** Settles once every step asked for so far has ended. */
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  /** Synchronous twin of `marking`: two clicks in one frame must not mark twice. */
  const markingNow = useRef(false);
  const { handle } = document;
  const allowed = document.info.annotationEditingAllowed && !denied;

  /** Runs `steps` after every step already asked for; stops at the first refusal. */
  const enqueue = useCallback(
    (steps: readonly (() => Promise<EditedDocument>)[]): Promise<void> => {
      setRunning((count) => count + 1);
      const run = queue.current.then(async () => {
        setFailure(null);
        try {
          for (const step of steps) onEdited(await step());
        } catch (error) {
          if (errorKind(error) === EDIT_NOT_PERMITTED) setDenied(true);
          setFailure(errorMessage(error));
        } finally {
          setRunning((count) => count - 1);
        }
      });
      queue.current = run;
      return run;
    },
    [onEdited],
  );

  /** One `addMarkup` per line of a selection on page `pageIndex`: one undo step each. */
  const markup = useCallback(
    async (kind: MarkupKind, pageIndex: number, rects: PageRects) => {
      if (!allowed || markingNow.current) return;
      const edits = markupEdits(pageIndex, rects, kind, MARKUP_COLOR);
      if (edits.length === 0) return;
      markingNow.current = true;
      setMarking(true);
      try {
        await enqueue(edits.map((edit) => () => engine.applyEdit(handle, edit)));
      } finally {
        markingNow.current = false;
        setMarking(false);
      }
    },
    [allowed, enqueue, engine, handle],
  );

  const canUndo = !!document.canUndo;
  const canRedo = !!document.canRedo;
  const undo = useCallback(
    () => (canUndo ? enqueue([() => engine.undo(handle)]) : Promise.resolve()),
    [canUndo, enqueue, engine, handle],
  );
  const redo = useCallback(
    () => (canRedo ? enqueue([() => engine.redo(handle)]) : Promise.resolve()),
    [canRedo, enqueue, engine, handle],
  );

  return { allowed, pending: running > 0, marking, failure, canUndo, canRedo, markup, undo, redo };
}
