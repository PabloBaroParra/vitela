// Container of the edits of several content items at once (B26 T-262): with a
// group selected (`useContentGroup`), drag it or nudge it with the arrow keys
// (1 pt, 10 pt with Shift) — every member moves together — or delete it.
// Each one undo step through the viewer's edit queue
// (`useAnnotationEdits.apply`), which marks the document dirty and that a
// save waits for.
//
// The rules are the core's (`moveContentSelection`, `removeContentSelection`):
// a second move of the same selection folds into its step; a selection that
// is exactly what a pending paste added moves the paste itself; part of one
// is refused until a save. Here, as for one item: nothing is asked while a
// step runs or the moved group is still to be found again (`busy`) — arrow
// presses meanwhile add up into one move — and each step checks, when its
// turn comes, that the page is still the one the group was selected on.

import { useCallback } from 'react';
import { pageKey } from '../annotations/useAnnotationEdits';
import type { DocumentInfo } from '@vitela/core';
import { useGuardedApply, useHeldMove, type ContentQueue } from './contentEdits';
import { membersOf } from './selection';
import { nudgeOf } from './textRuns';
import type { ContentGroupState } from './useContentGroup';

const PAGE_CHANGED = 'The page changed before the selection could be edited. Nothing was changed.';

interface GroupEditingInput {
  readonly info: Pick<DocumentInfo, 'pages' | 'pageIds'>;
  readonly group: ContentGroupState;
  readonly queue: ContentQueue;
  /** Whether a copy for a cut runs: the cut removes the selection itself. */
  readonly clipping?: boolean;
}

export function useGroupEditing({ info, group: selection, queue, clipping = false }: GroupEditingInput) {
  const { group, followMoved, clear } = selection;
  const busy = queue.pending || selection.following || clipping;
  const guarded = useGuardedApply(queue, PAGE_CHANGED);

  const send = useCallback(
    (dx: number, dy: number) => {
      if (!group || (dx === 0 && dy === 0)) return;
      const page = group.pageIndex;
      followMoved(group, dx, dy);
      guarded(page, pageKey(info, page), { kind: 'moveContentSelection', page, members: membersOf(group.items), dx, dy });
    },
    [group, followMoved, guarded, info],
  );
  const move = useHeldMove(busy, !!group || selection.following, group, send);

  const remove = useCallback(() => {
    if (!group || busy) return;
    const page = group.pageIndex;
    clear();
    guarded(page, pageKey(info, page), { kind: 'removeContentSelection', page, members: membersOf(group.items) });
  }, [group, busy, clear, guarded, info]);

  /** The keys the pages take for the group; `true` when one was used. */
  const onKey = useCallback(
    (key: string, shift: boolean): boolean => {
      if (!group) {
        // Only an arrow, and only while the moved group is being found again.
        const nudge = selection.following ? nudgeOf(key, shift) : null;
        if (nudge) move(nudge.dx, nudge.dy);
        return nudge !== null;
      }
      if (key === 'Delete' || key === 'Backspace') {
        remove();
        return true;
      }
      if (key === 'Escape') {
        clear();
        return true;
      }
      const nudge = nudgeOf(key, shift);
      if (!nudge) return false;
      move(nudge.dx, nudge.dy);
      return true;
    },
    [group, selection.following, move, remove, clear],
  );

  return { busy, move, remove, onKey };
}

export type GroupEditing = ReturnType<typeof useGroupEditing>;
