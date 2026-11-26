// The Organize screen's edits: sends one page edit, undo or redo at a time to
// the engine, and reports the edited document. Container logic only.
//
// The rules are the core's: whether the document permits page assembly is
// known at open (`documentAssemblyAllowed`) and is asked again by the core
// on every edit (`EditNotPermitted`), exactly as `TextExtractionDenied`
// works for text. The engine refreshes its preview inside the edit, so the
// next render already shows it.
//
// The report (`onEdited`) is made even if the screen went away while the
// edit ran: the core has the edit either way, so the workspace must learn
// the new page list. It is the workspace that drops an answer for a document
// no longer on screen.
//
// A page edit asked for while another is in flight is dropped (a double
// click must not send two). An import is not: it is the end of a pick the
// user may have typed passwords for, so it waits for the edit in flight and
// then runs, after the pages that edit left.

import { useCallback, useRef, useState } from 'react';
import { EDIT_NOT_PERMITTED, errorKind, errorMessage } from '@vitela/core';
import type { EditedDocument, Engine, ImportedDocument, PageEdit } from '@vitela/core';
import type { OpenDocument } from '../document/useDocument';
import { useSaveWaitsFor } from '../save/MarkDirtyContext';

export type OrganizeSession = ReturnType<typeof useOrganize>;

export function useOrganize(engine: Engine, document: OpenDocument, onEdited: (edited: EditedDocument) => void) {
  const [pending, setPending] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [denied, setDenied] = useState(false);
  /** Synchronous twin of `pending`: two clicks in one frame must not send two edits. */
  const busy = useRef(false);
  /** Settles once the step in flight has ended, whatever its outcome. */
  const settled = useRef<Promise<unknown>>(Promise.resolve());
  // Save and the tools that write a copy (Compress, Protect, Export, Extract,
  // Split) wait for the step in flight, as they wait for the viewer's edits.
  const settledNow = useCallback(() => settled.current, []);
  useSaveWaitsFor(settledNow);
  const { handle } = document;
  const allowed = document.info.documentAssemblyAllowed && !denied;

  /** The page count as of the last render: an import appends after the pages there are now. */
  const pageCount = useRef(document.info.pageCount);
  pageCount.current = document.info.pageCount;

  /**
   * Runs `step` unless another is in flight. `clearFailure` is false for a
   * step that waited its turn: the failure of the one it waited for stays on
   * show, as the user has not seen it yet.
   */
  const run = useCallback(
    <Edited extends EditedDocument>(step: () => Promise<Edited>, clearFailure = true): Promise<Edited | null> => {
      if (busy.current) return Promise.resolve(null);
      busy.current = true;
      setPending(true);
      if (clearFailure) setFailure(null);
      const running = (async () => {
        try {
          const edited = await step();
          // The next step may start before a render brings the new page list.
          pageCount.current = edited.info.pageCount;
          onEdited(edited);
          return edited;
        } catch (error) {
          if (errorKind(error) === EDIT_NOT_PERMITTED) setDenied(true);
          setFailure(errorMessage(error));
          return null;
        } finally {
          busy.current = false;
          setPending(false);
        }
      })();
      settled.current = running;
      return running;
    },
    [onEdited],
  );

  /** Runs `step` once the step in flight, if any, has ended: never dropped. */
  const runInTurn = useCallback(
    async <Edited extends EditedDocument>(step: () => Promise<Edited>): Promise<Edited | null> => {
      let waited = false;
      while (busy.current) {
        waited = true;
        await settled.current;
      }
      return run(step, !waited);
    },
    [run],
  );

  const apply = useCallback((edit: PageEdit) => run(() => engine.applyEdit(handle, edit)), [run, engine, handle]);
  /**
   * Adds the prepared import sources `ids` after the last page, as one step;
   * after the edit in flight, if there is one.
   */
  const importPrepared = useCallback(
    (ids: readonly number[]): Promise<ImportedDocument | null> =>
      runInTurn(() => engine.importPrepared(handle, ids, pageCount.current)),
    [runInTurn, engine, handle],
  );
  const canUndo = !!document.canUndo;
  const canRedo = !!document.canRedo;
  const undo = useCallback(
    () => (canUndo ? run(() => engine.undo(handle)) : Promise.resolve(null)),
    [run, engine, handle, canUndo],
  );
  const redo = useCallback(
    () => (canRedo ? run(() => engine.redo(handle)) : Promise.resolve(null)),
    [run, engine, handle, canRedo],
  );

  return { allowed, pending, failure, canUndo, canRedo, apply, importPrepared, undo, redo };
}
