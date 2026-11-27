// Container of the "Edit content" mode's several-item selection (B26 T-262):
// what a marquee drag over a page or a SHIFT/CTRL+click gathers there — text
// runs, images, shapes — held by the read it was made in, and found again
// once an edit of it (a move, a paste) has landed and the page is read anew.
//
// The single selection stays `useContentMode`'s, and every edit of one item
// works on it as before: a group of one made by a press or a marquee is that
// single selection, so its toolbar and handles show. A paste is the
// exception, as in the GTK and Windows shells: what it added is a group
// (dragging it moves the paste itself), unless it is one image alone, which
// is selected the way a click selects it (Replace, Delete).

import { useCallback, useEffect, useState } from 'react';
import type { ContentItem, PageContent, PagePoint, PageRect } from '@vitela/core';
import type { ContentMode } from './useContentMode';
import { describeGroup, followGroup, gathered, pastedItems, soughtOf, toggled, type Sought } from './selection';

/** Several items of one page, with the read they were found in. */
export interface ContentGroup {
  readonly pageIndex: number;
  readonly key: string;
  readonly items: readonly ContentItem[];
}

/** What an edit of the group leaves to select again once its page is read anew. */
type Target =
  | { readonly kind: 'moved'; readonly sought: readonly Sought[] }
  | { readonly kind: 'pasted'; readonly landed: PageRect; readonly before: PageContent | null };

interface GroupFollow {
  readonly pageIndex: number;
  /** The key its page had when the edit was asked for. */
  readonly fromKey: string;
  readonly target: Target;
  /** Whether a step was seen running since: until then, an unchanged key means nothing yet. */
  readonly seenPending: boolean;
}

interface ContentGroupInput {
  readonly content: ContentMode;
  /** Whether a step of the viewer's edit queue is running. */
  readonly paused: boolean;
}

export function useContentGroup({ content, paused }: ContentGroupInput) {
  const [group, setGroup] = useState<ContentGroup | null>(null);
  const [follow, setFollow] = useState<GroupFollow | null>(null);
  const { keyAt, stateAt, choose, clear: clearSingle, wants, active } = content;

  /**
   * Holds `items` of page `pageIndex` (read `key`) as the selection: none
   * clears it, one is the single selection unless `asGroup`, more are a group.
   */
  const settle = useCallback(
    (pageIndex: number, key: string, items: readonly ContentItem[], asGroup = false) => {
      const [only] = items;
      if (only && items.length === 1 && !asGroup) {
        setGroup(null);
        choose({ pageIndex, key, item: only });
        return;
      }
      clearSingle();
      setGroup(items.length > 0 ? { pageIndex, key, items } : null);
    },
    [choose, clearSingle],
  );

  const shown = active && group && keyAt(group.pageIndex) === group.key ? group : null;
  /** What is selected on page `pageIndex` now: the group, or the single item. */
  const selectionOn = useCallback(
    (pageIndex: number): readonly ContentItem[] => {
      if (shown) return shown.pageIndex === pageIndex ? shown.items : [];
      const single = content.selected;
      return single && single.pageIndex === pageIndex ? [single.item] : [];
    },
    [shown, content.selected],
  );

  /** A SHIFT/CTRL+click: the item under `point` joins the selection on its page, or leaves it. */
  const toggle = useCallback(
    (pageIndex: number, point: PagePoint) => {
      const picked = content.itemAt(pageIndex, point);
      if (!picked) return;
      setFollow(null);
      settle(pageIndex, picked.key, toggled(selectionOn(pageIndex), picked.item));
    },
    [content, settle, selectionOn],
  );

  /** A marquee over `area` of page `pageIndex`: what it touches is selected, added to what is (`additive`). */
  const gather = useCallback(
    (pageIndex: number, area: PageRect, additive: boolean) => {
      const key = keyAt(pageIndex);
      const state = stateAt(pageIndex);
      if (key === undefined || state?.status !== 'ready') return;
      setFollow(null);
      let items = additive ? [...selectionOn(pageIndex)] : [];
      for (const item of gathered(state.content, area)) if (!items.some((member) => member.kind === item.kind && member.id === item.id)) items = [...items, item];
      settle(pageIndex, key, items);
    },
    [keyAt, stateAt, settle, selectionOn],
  );

  const clear = useCallback(() => {
    setGroup(null);
    setFollow(null);
  }, []);
  // Leaving the mode lets it go.
  useEffect(() => {
    if (!active) clear();
  }, [active, clear]);

  /** Selects the group again where a move by `dx, dy` puts it. Call it with the edit, before the step lands. */
  const followMoved = useCallback(
    (moved: ContentGroup, dx: number, dy: number) => {
      setFollow({ pageIndex: moved.pageIndex, fromKey: moved.key, target: { kind: 'moved', sought: soughtOf(moved.items, dx, dy) }, seenPending: false });
    },
    [],
  );
  /** Selects what a paste added at `landed` on page `pageIndex`, once it is read. Call it with the paste. */
  const followPasted = useCallback(
    (pageIndex: number, landed: PageRect) => {
      const fromKey = keyAt(pageIndex);
      // What was pending before it, so only what the paste itself added is selected.
      const state = stateAt(pageIndex);
      const before = state?.status === 'ready' ? state.content : null;
      setFollow(fromKey === undefined ? null : { pageIndex, fromKey, target: { kind: 'pasted', landed, before }, seenPending: false });
    },
    [keyAt, stateAt],
  );

  useEffect(() => {
    if (!follow) return;
    // A page the mode no longer reads would never be read again for it.
    if (!active || !wants(follow.pageIndex)) return void setFollow(null);
    if (paused) {
      if (!follow.seenPending) setFollow({ ...follow, seenPending: true });
      return;
    }
    const key = keyAt(follow.pageIndex);
    // The edit changed nothing (or was refused): what was selected still holds.
    if (key === follow.fromKey) {
      if (follow.seenPending) setFollow(null);
      return;
    }
    const state = stateAt(follow.pageIndex);
    if (key !== undefined && (state === undefined || state.status === 'loading')) return;
    setFollow(null);
    if (key === undefined || state?.status !== 'ready') return;
    const { target } = follow;
    if (target.kind === 'moved') {
      // A group stays one, even of one item: dragging it again moves a paste as a whole.
      settle(follow.pageIndex, key, followGroup(state.content, target.sought), true);
      return;
    }
    const pasted = pastedItems(state.content, target.landed, target.before);
    settle(follow.pageIndex, key, pasted, !(pasted.length === 1 && pasted[0]!.kind === 'image'));
  }, [follow, active, wants, paused, keyAt, stateAt, settle]);

  return {
    group: shown,
    /** Whether an edit of the group is still to be found again (its page not read yet). */
    following: follow !== null,
    /** What the status line says about the group; `null` without one. */
    status: shown ? describeGroup(shown.items.length, shown.pageIndex) : null,
    toggle,
    gather,
    clear,
    followMoved,
    followPasted,
  };
}

export type ContentGroupState = ReturnType<typeof useContentGroup>;
