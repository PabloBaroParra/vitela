// Container of the "Edit content" mode's Insert text (B26 T-261): armed from
// the status row, the next press on a page opens an empty editor there, and
// what is typed in it is recorded as one new line of page content — one undo
// step through the viewer's edit queue (`useAnnotationEdits.apply`), which
// marks the document dirty and that a save waits for.
//
// As in the GTK and Windows shells, the box writes exactly once: on Enter,
// on leaving it, or when a save, leaving the mode, another press or a scroll
// that unmounts its page commits it; Escape records nothing, and an empty box
// is a press that changed its mind. The line is 14 pt Helvetica, its top-left
// corner at the press (`INSERT_TEXT_SIZE_PT`; the core picks the font and
// measures the line). One press per arming, like Insert image; Escape at the
// pages disarms. Once recorded, the new line is selected where it lands: it
// can be retyped, moved or deleted like any other run.

import { useCallback, useEffect, useState } from 'react';
import type { DocumentInfo, PagePoint } from '@vitela/core';
import { pageKey } from '../annotations/useAnnotationEdits';
import { useGuardedApply, type ContentQueue } from './contentEdits';
import { INSERT_TEXT_PROMPT, INSERT_TEXT_SIZE_PT, insertedRect, insertionOf } from './textBlocks';
import type { ContentMode } from './useContentMode';
import { useDraftEditor, type Drafted } from './useDraftEditor';

/** Where the new line goes: the page position, the press, and the page it was pressed on. */
export interface InsertTarget {
  readonly pageIndex: number;
  readonly at: PagePoint;
  readonly pageKey: string;
}

export type InsertEditor = Drafted<InsertTarget>;

const PAGE_CHANGED = 'The page changed before the text could be added. Nothing was added.';

interface TextInsertionInput {
  readonly info: Pick<DocumentInfo, 'pages' | 'pageIds'>;
  readonly content: Pick<ContentMode, 'active' | 'follow'>;
  readonly queue: ContentQueue;
  /** Gives the keyboard back to the pages. */
  readonly focusPages: () => void;
}

export function useTextInsertion({ info, content, queue, focusPages }: TextInsertionInput) {
  const [armed, setArmed] = useState(false);
  const guarded = useGuardedApply(queue, PAGE_CHANGED);
  const { follow } = content;
  const record = useCallback(
    ({ pageIndex, at, pageKey: expected, draft }: InsertEditor) => {
      const edit = insertionOf(pageIndex, at, draft);
      if (!edit) return;
      const rotation = info.pages[pageIndex]?.rotationDegrees ?? 0;
      follow(pageIndex, insertedRect(at, draft, INSERT_TEXT_SIZE_PT, rotation), draft);
      guarded(pageIndex, expected, edit);
    },
    [info.pages, follow, guarded],
  );
  const draft = useDraftEditor<InsertTarget>({ active: content.active, onCommit: record, focusPages });

  const arm = useCallback(() => {
    setArmed(true);
    // The button keeps the focus otherwise, and Escape is the pages'.
    focusPages();
  }, [focusPages]);
  const disarm = useCallback(() => setArmed(false), []);
  // Leaving the mode disarms (and the draft editor commits what was typed).
  useEffect(() => {
    if (!content.active) setArmed(false);
  }, [content.active]);

  const { open } = draft;
  /** A press on page `pageIndex` while armed: the editor opens there. `false` when not armed. */
  const place = useCallback(
    (pageIndex: number, at: PagePoint): boolean => {
      if (!armed) return false;
      setArmed(false);
      open({ pageIndex, at, pageKey: pageKey(info, pageIndex) }, '');
      return true;
    },
    [armed, open, info],
  );

  /** The keys the pages take for Insert text; `true` when one was used. */
  const onKey = useCallback(
    (key: string): boolean => {
      if (!armed || key !== 'Escape') return false;
      setArmed(false);
      return true;
    },
    [armed],
  );

  return {
    armed,
    /** What the status row says while armed; `null` otherwise. */
    prompt: armed ? INSERT_TEXT_PROMPT : null,
    arm,
    disarm,
    place,
    onKey,
    editor: draft.editor,
    commit: draft.commit,
    cancel: draft.cancel,
    typing: draft.typing,
    commitOpen: draft.commitOpen,
    attach: draft.attach,
    detach: draft.detach,
  };
}

export type TextInsertion = ReturnType<typeof useTextInsertion>;
