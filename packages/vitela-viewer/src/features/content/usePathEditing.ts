// Container of the "Edit content" mode's vector-path edits (B26 T-263): with
// a rule, a border or a shaded box selected, move it (arrow keys, or a drag
// of its body), resize it (a handle) and delete it. Each one undo step
// through the viewer's edit queue (`useAnnotationEdits.apply`), which marks
// the document dirty and that a save waits for. The twin of
// `useImageEditing`, without the picture.
//
// The rules are the core's (`applyEdit` with a `PathEdit`): the gate, the
// fold of a second move or resize of the same path into its step, a resize
// that keeps the line width and leaves a flat side flat. Here: what to ask
// for and when.
//
// - An edit names the path by its id in the read it was selected in, so
//   none is asked for while another step runs, an edited item is still to
//   be found again, or a cut is still being read (`busy`): arrow presses
//   meanwhile add up and go as one move once the path is found again
//   (`ContentMode.followPath`).
// - Each step checks, when its turn comes, that the page at the path's
//   position is still the one it was selected on (`pageKey`).

import { useCallback } from 'react';
import type { DocumentInfo, Edit, PageRect } from '@vitela/core';
import { pageKey } from '../annotations/useAnnotationEdits';
import { useGuardedApply, useHeldMove, type ContentQueue } from './contentEdits';
import { sameRect } from './contentModel';
import type { ContentMode } from './useContentMode';
import { nudgeOf } from './textRuns';

const PAGE_CHANGED = 'The page changed before the shape could be edited. Nothing was changed.';

interface PathEditingInput {
  readonly info: Pick<DocumentInfo, 'pages' | 'pageIds'>;
  readonly content: ContentMode;
  readonly queue: ContentQueue;
  /** Whether a copy for a cut is still being read: it removes the selection itself once read. */
  readonly clipping?: boolean;
}

export function usePathEditing({ info, content, queue, clipping = false }: PathEditingInput) {
  const picked = content.selected;
  const path = picked?.item.kind === 'path' ? picked.item : null;
  const busy = queue.pending || content.following || clipping;
  const currentPageKey = picked ? pageKey(info, picked.pageIndex) : null;

  const guarded = useGuardedApply(queue, PAGE_CHANGED);
  const { followPath } = content;
  /** Records `edit` as a step, selecting again the path now at `expected` (`null`: none). */
  const submit = useCallback(
    (pageIndex: number, expectedPageKey: string, edit: Edit, expected: PageRect | null) => {
      followPath(pageIndex, expected);
      guarded(pageIndex, expectedPageKey, edit);
    },
    [followPath, guarded],
  );

  // Arrow presses while busy add up into one move, sent once the path is found again.
  const send = useCallback(
    (dx: number, dy: number) => {
      if (!path || currentPageKey === null || (dx === 0 && dy === 0)) return;
      const { rect } = path;
      submit(path.pageIndex, currentPageKey, { kind: 'movePath', page: path.pageIndex, pathId: path.id, dx, dy }, {
        ...rect,
        left: rect.left + dx,
        top: rect.top + dy,
      });
    },
    [path, currentPageKey, submit],
  );
  const move = useHeldMove(busy, !!path || content.followingKind === 'path', path, send);

  const resize = useCallback(
    (rect: PageRect) => {
      if (!path || currentPageKey === null || busy || sameRect(rect, path.rect)) return;
      submit(path.pageIndex, currentPageKey, { kind: 'resizePath', page: path.pageIndex, pathId: path.id, rect }, rect);
    },
    [path, currentPageKey, busy, submit],
  );

  const remove = useCallback(() => {
    if (!path || currentPageKey === null || busy) return;
    submit(path.pageIndex, currentPageKey, { kind: 'removePath', page: path.pageIndex, pathId: path.id }, null);
  }, [path, currentPageKey, busy, submit]);

  /** The keys the pages take for the path edits; `true` when one was used. */
  const onKey = useCallback(
    (key: string, shift: boolean): boolean => {
      if (!path) {
        // Only an arrow, and only while the path it moves is being found again.
        const nudge = content.followingKind === 'path' ? nudgeOf(key, shift) : null;
        if (nudge) move(nudge.dx, nudge.dy);
        return nudge !== null;
      }
      if (key === 'Delete' || key === 'Backspace') {
        remove();
        return true;
      }
      const nudge = nudgeOf(key, shift);
      if (!nudge) return false;
      move(nudge.dx, nudge.dy);
      return true;
    },
    [path, content.followingKind, move, remove],
  );

  return { path, busy, move, resize, remove, onKey };
}

export type PathEditing = ReturnType<typeof usePathEditing>;
