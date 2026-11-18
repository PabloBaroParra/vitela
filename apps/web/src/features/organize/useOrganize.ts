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

import { useCallback, useRef, useState } from 'react';
import { EDIT_NOT_PERMITTED, errorKind, errorMessage } from '@vitela/core';
import type { EditedDocument, Engine, ImportedDocument, PageEdit } from '@vitela/core';
import type { OpenDocument } from '../document/useDocument';

export type OrganizeSession = ReturnType<typeof useOrganize>;

export function useOrganize(engine: Engine, document: OpenDocument, onEdited: (edited: EditedDocument) => void) {
  const [pending, setPending] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [denied, setDenied] = useState(false);
  /** Synchronous twin of `pending`: two clicks in one frame must not send two edits. */
  const busy = useRef(false);
  const { handle } = document;
  const allowed = document.info.documentAssemblyAllowed && !denied;

  /** The page count as of the last render: an import appends after the pages there are now. */
  const pageCount = useRef(document.info.pageCount);
  pageCount.current = document.info.pageCount;

  const run = useCallback(
    async <Edited extends EditedDocument>(step: () => Promise<Edited>): Promise<Edited | null> => {
      if (busy.current) return null;
      busy.current = true;
      setPending(true);
      setFailure(null);
      try {
        const edited = await step();
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
    },
    [onEdited],
  );

  const apply = useCallback((edit: PageEdit) => run(() => engine.applyEdit(handle, edit)), [run, engine, handle]);
  /** Adds the prepared import sources `ids` after the last page, as one step. */
  const importPrepared = useCallback(
    (ids: readonly number[]): Promise<ImportedDocument | null> =>
      run(() => engine.importPrepared(handle, ids, pageCount.current)),
    [run, engine, handle],
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
