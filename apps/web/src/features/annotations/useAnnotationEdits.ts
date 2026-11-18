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
// three-line markup is still being added waits for its last line. A markup
// is refused while any step is still running (an undo may be putting
// another page where the selection was), and is checked once more right
// before its first line goes in: if the page at its position is no longer
// the one it was selected on (`pageKey`), nothing is marked. The report
// (`onEdited`) is made even if the viewer went away meanwhile: the core has
// the edit either way.

import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import { EDIT_NOT_PERMITTED, errorKind, errorMessage } from '@vitela/core';
import type { DocumentInfo, EditedDocument, Engine, MarkupKind, PageRects, Rgb } from '@vitela/core';
import type { OpenDocument } from '../document/useDocument';
import { markupEdits } from './geometry';

/** The markup colour of the native shells (their `DEFAULT_COLOR`). */
export const MARKUP_COLOR: Rgb = { r: 255, g: 220, b: 0 };

type PageLayout = Pick<DocumentInfo, 'pages' | 'pageIds'>;

/**
 * What sits at position `index`: the page and how it is turned. Two equal
 * keys mean a selection made on one still fits the other.
 */
export function pageKey(info: PageLayout, index: number): string {
  return `${info.pageIds[index] ?? index}/${info.pages[index]?.rotationDegrees ?? 0}`;
}

const PAGE_CHANGED = 'The page changed before it could be marked up. Select the text again.';

/**
 * How a markup ended: every line went in (`marked`), some did before one
 * failed (`partial`), none did (`failed`), or nothing was asked of the core
 * (`refused`: not allowed, a step still running, no lines). `partial` and
 * `failed` leave a `failure` to show; `refused` does not.
 */
export type MarkupOutcome = 'marked' | 'partial' | 'failed' | 'refused';

/** How a run of queued steps ended: how many went in, and whether all did. */
interface QueueResult {
  readonly applied: number;
  readonly complete: boolean;
}

/** A step that must not run: the page it was meant for has moved. */
class PageChanged extends Error {}

export function useAnnotationEdits(engine: Engine, document: OpenDocument, onEdited: (edited: EditedDocument) => void) {
  const [running, setRunning] = useState(0);
  const [failure, setFailure] = useState<string | null>(null);
  const [denied, setDenied] = useState(false);
  /** Settles once every step asked for so far has ended. */
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  /** Synchronous twin of `running`: two clicks in one frame must not both get in. */
  const runningNow = useRef(0);
  /**
   * The pages as the core last described them: from each step's result as
   * soon as it lands, ahead of the re-render that brings them as a prop.
   */
  const latest = useRef<PageLayout>(document.info);
  useLayoutEffect(() => {
    latest.current = document.info;
  }, [document.info]);
  const { handle } = document;
  const allowed = document.info.annotationEditingAllowed && !denied;

  /** Runs `steps` after every step already asked for; stops at the first refusal. */
  const enqueue = useCallback(
    (steps: readonly (() => Promise<EditedDocument>)[]): Promise<QueueResult> => {
      runningNow.current += 1;
      setRunning((count) => count + 1);
      const run = queue.current.then(async (): Promise<QueueResult> => {
        setFailure(null);
        let applied = 0;
        try {
          for (const step of steps) {
            const edited = await step();
            applied += 1;
            latest.current = edited.info;
            onEdited(edited);
          }
          return { applied, complete: true };
        } catch (error) {
          if (error instanceof PageChanged) {
            setFailure(PAGE_CHANGED);
          } else {
            if (errorKind(error) === EDIT_NOT_PERMITTED) setDenied(true);
            setFailure(errorMessage(error));
          }
          return { applied, complete: false };
        } finally {
          runningNow.current -= 1;
          setRunning((count) => count - 1);
        }
      });
      queue.current = run;
      return run;
    },
    [onEdited],
  );

  /**
   * One `addMarkup` per line of a selection on page `pageIndex`, which held
   * the page `expectedKey` (see `pageKey`) when it was selected: one undo
   * step each. See `MarkupOutcome` for how it ends.
   */
  const markup = useCallback(
    async (kind: MarkupKind, pageIndex: number, expectedKey: string, rects: PageRects): Promise<MarkupOutcome> => {
      if (!allowed || runningNow.current > 0) return 'refused';
      const edits = markupEdits(pageIndex, rects, kind, MARKUP_COLOR);
      if (edits.length === 0) return 'refused';
      const { applied, complete } = await enqueue(
        edits.map((edit, line) => () => {
          if (line === 0 && pageKey(latest.current, pageIndex) !== expectedKey) {
            return Promise.reject(new PageChanged());
          }
          return engine.applyEdit(handle, edit);
        }),
      );
      if (complete) return 'marked';
      return applied > 0 ? 'partial' : 'failed';
    },
    [allowed, enqueue, engine, handle],
  );

  const canUndo = !!document.canUndo;
  const canRedo = !!document.canRedo;
  const undo = useCallback(async () => {
    if (canUndo) await enqueue([() => engine.undo(handle)]);
  }, [canUndo, enqueue, engine, handle]);
  const redo = useCallback(async () => {
    if (canRedo) await enqueue([() => engine.redo(handle)]);
  }, [canRedo, enqueue, engine, handle]);

  return { allowed, pending: running > 0, failure, canUndo, canRedo, markup, undo, redo };
}
