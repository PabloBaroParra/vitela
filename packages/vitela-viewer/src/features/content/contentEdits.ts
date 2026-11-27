// What the "Edit content" mode's edit containers share (`useTextRunEditing`,
// `useImageEditing`): the viewer's edit queue they record their steps on,
// the guard each step runs at its turn, and the arrow presses held while a
// step runs.

import { useCallback, useEffect, useRef } from 'react';
import type { Edit } from '@vitela/core';
import { pageKey, type Session } from '../annotations/useAnnotationEdits';

/** What the content edits need of the viewer's edit queue (`useAnnotationEdits`). */
export interface ContentQueue {
  readonly pending: boolean;
  readonly apply: (build: (session: Session) => Edit | null) => Promise<unknown>;
  readonly notify: (message: string) => void;
}

/**
 * Records `edit` as one step that, when its turn comes, checks the page at
 * `pageIndex` is still the one the item was selected on (`expectedPageKey`):
 * refused with `pageChanged` otherwise.
 */
export function useGuardedApply(queue: ContentQueue, pageChanged: string) {
  const { apply } = queue;
  return useCallback(
    (pageIndex: number, expectedPageKey: string, edit: Edit) => {
      void apply((session) => {
        if (pageKey(session.info, pageIndex) !== expectedPageKey) throw new Error(pageChanged);
        return edit;
      });
    },
    [apply, pageChanged],
  );
}

const STILL = { dx: 0, dy: 0 };

/**
 * A move of the selected item by `send`, with the presses made while a step
 * runs (`busy`) held and added up when they are for the item (`holding`: it
 * is selected, or being found again), then sent as one move once nothing
 * runs — if the item was found again (`target`); a selection lost meanwhile
 * drops them.
 */
export function useHeldMove(busy: boolean, holding: boolean, target: unknown, send: (dx: number, dy: number) => void) {
  const held = useRef(STILL);
  const move = useCallback(
    (dx: number, dy: number) => {
      if (!busy) return send(dx, dy);
      // While the moved item is found again its old pick no longer holds,
      // but the presses meanwhile are still for it.
      if (holding) held.current = { dx: held.current.dx + dx, dy: held.current.dy + dy };
    },
    [busy, holding, send],
  );
  useEffect(() => {
    if (busy) return;
    const { dx, dy } = held.current;
    if (dx === 0 && dy === 0) return;
    held.current = STILL;
    if (target) send(dx, dy);
  }, [busy, target, send]);
  return move;
}
