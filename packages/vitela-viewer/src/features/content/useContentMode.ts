// Containers of the "Edit content" mode (B26 T-258b), read-only for now.
//
// `useContentGate` asks the core once per document whether content may be
// edited at all (the core's sentence when not): the workspace asks it, since
// the mode is turned on from its Tools menu. `useContentMode` is the viewer's:
// while the mode is on it parses the pages in view and one on each side —
// never more (see `contentModel`) — holds those parses by page id, paint
// revision and base generation, and keeps the item under the pointer and the
// one clicked.
//
// No edit is made here yet (T-259 on): what it hands the pages is what to
// outline, and what it hands the status line is what was selected.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { errorMessage, type ContentItem, type DocumentHandle, type Engine, type PagePoint, type PageRect } from '@vitela/core';
import { sentence } from '../../ui/text';
import { followRun } from './textRuns';
import {
  CONTENT_HINT,
  contentKey,
  describeItem,
  itemAt,
  keepWanted,
  pagesToParse,
  readsToSend,
  sameItem,
  type PageContentState,
  type WantedPage,
} from './contentModel';

/** The item under the pointer, or the selected one, with the parse it was found in. */
interface Picked {
  readonly pageIndex: number;
  readonly key: string;
  readonly item: ContentItem;
}

/** The run an edit is waiting to select again. */
interface Following {
  readonly pageIndex: number;
  /** The key its page had when the edit was asked for. */
  readonly fromKey: string;
  readonly expected: PageRect;
  readonly text: string | null;
  /** Whether an edit was seen running since: until then, an unchanged key means nothing yet. */
  readonly seenPending: boolean;
}

/** Whether the mode may be offered: still asking, yes, or the core's sentence. */
export type ContentGate = { readonly status: 'checking' } | { readonly status: 'allowed' } | { readonly status: 'refused'; readonly reason: string };

/** Whether `handle`'s content may be edited, asked once per document; `checking` with none open. */
export function useContentGate(engine: Engine, handle: DocumentHandle | null): ContentGate {
  const [gate, setGate] = useState<{ readonly handle: DocumentHandle; readonly gate: ContentGate } | null>(null);
  useEffect(() => {
    if (handle === null) return;
    let live = true;
    engine.contentEditingRefusal(handle).then(
      (reason) => live && setGate({ handle, gate: reason === null ? { status: 'allowed' } : { status: 'refused', reason: sentence(reason) } }),
      (error: unknown) => live && setGate({ handle, gate: { status: 'refused', reason: sentence(errorMessage(error)) } }),
    );
    return () => {
      live = false;
    };
  }, [engine, handle]);
  return gate !== null && gate.handle === handle ? gate.gate : { status: 'checking' };
}

interface ContentModeInput {
  readonly engine: Engine;
  readonly handle: DocumentHandle;
  /** Whether the mode is on (the workspace's, already gated). */
  readonly active: boolean;
  /** The page id at each position. */
  readonly pageIds: readonly number[];
  /** The clockwise turn of the page at each position (missing: upright). */
  readonly rotations: readonly number[];
  /**
   * Whether an edit is still running: the core answers reads in the order
   * they are asked, so one asked now would read the page an Organize move or
   * an undo is about to put at that position. Nothing is read meanwhile.
   */
  readonly paused: boolean;
  readonly paintRevision: number;
  readonly baseGeneration: number;
  /** The positions on screen now (no overscan), or `null` before the first layout. */
  readonly visible: { readonly first: number; readonly last: number } | null;
}

export function useContentMode({
  engine,
  handle,
  active,
  pageIds,
  rotations,
  paused,
  paintRevision,
  baseGeneration,
  visible,
}: ContentModeInput) {
  const keyAt = useCallback(
    (index: number) => {
      const id = pageIds[index];
      return id === undefined ? undefined : contentKey(id, rotations[index] ?? 0, paintRevision, baseGeneration);
    },
    [pageIds, rotations, paintRevision, baseGeneration],
  );
  // Read through a ref: a read landing checks the page at its position now.
  const keyNow = useRef(keyAt);
  useLayoutEffect(() => {
    keyNow.current = keyAt;
  });

  // What to parse: the pages in view and one on each side, by position and key.
  const wanted = useMemo<WantedPage[]>(() => {
    if (!active) return [];
    return pagesToParse(visible, pageIds.length).flatMap((index) => {
      const key = keyAt(index);
      return key === undefined ? [] : [{ index, key }];
    });
  }, [active, visible, pageIds.length, keyAt]);
  const wantedSignature = wanted.map(({ index, key }) => `${index}:${key}`).join(',');

  const [held, setHeld] = useState<ReadonlyMap<string, PageContentState>>(() => new Map());
  /** The keys whose read the core has not answered yet. */
  const inFlight = useRef(new Set<string>());
  /** Bumped when a read lands: what to send next is decided again then. */
  const [landed, setLanded] = useState(0);
  useEffect(() => {
    const keys = new Set(wanted.map(({ key }) => key));
    // Drop what is no longer wanted, and mark what is to be parsed.
    if (wanted.some(({ key }) => !held.has(key)) || [...held.keys()].some((key) => !keys.has(key))) {
      setHeld((current) => {
        const next = keepWanted(current, keys);
        for (const { key } of wanted) if (!next.has(key)) next.set(key, { status: 'loading' });
        return next;
      });
      return;
    }
    if (paused) return;
    for (const { index, key } of readsToSend(wanted, held, inFlight.current)) {
      inFlight.current.add(key);
      // A parse lands only on the key it was asked for, while that key is
      // still waiting for it, and while its position still holds the page it
      // was asked for: one moved meanwhile is read again where it is now.
      const land = (state: PageContentState) => {
        inFlight.current.delete(key);
        setLanded((count) => count + 1);
        if (keyNow.current(index) !== key) return;
        setHeld((current) => (current.get(key)?.status === 'loading' ? new Map(current).set(key, state) : current));
      };
      engine.readPageContent(handle, index).then(
        (content) => land({ status: 'ready', content }),
        (error: unknown) => land({ status: 'failed', message: sentence(errorMessage(error)) }),
      );
    }
    // `wanted` is read through its signature; `held` and `landed` are
    // watched so the next reads go out as soon as one lands.
  }, [engine, handle, wantedSignature, held, paused, landed]);

  const [hovered, setHovered] = useState<Picked | null>(null);
  const [selected, setSelected] = useState<Picked | null>(null);
  // Leaving the mode, or another document, lets both go.
  useEffect(() => {
    if (active) return;
    setHovered(null);
    setSelected(null);
  }, [active]);

  // While an edit runs, the parse held for a page may name what it is about
  // to change (an undo taking a retype back): no page reads as ready, and a
  // press or the pointer picks nothing until it lands.
  const stateAt = useCallback(
    (index: number): PageContentState | undefined => {
      const key = keyAt(index);
      return key === undefined || paused ? undefined : held.get(key);
    },
    [keyAt, held, paused],
  );

  /** What is under `point` on page `index`, as the parse held now finds it. */
  const pick = useCallback(
    (index: number, point: PagePoint): Picked | null => {
      const key = keyAt(index);
      const state = key === undefined || paused ? undefined : held.get(key);
      if (key === undefined || state?.status !== 'ready') return null;
      const item = itemAt(state.content, point);
      return item && { pageIndex: index, key, item };
    },
    [keyAt, held, paused],
  );

  const hover = useCallback(
    (index: number, point: PagePoint | null) => {
      const next = point ? pick(index, point) : null;
      setHovered((shown) => (samePick(shown, next) ? shown : next));
    },
    [pick],
  );
  // After an edit of the selected run, the page is read again and the run is
  // found there by place (ids hold only for the read they came from).
  const [following, setFollowing] = useState<Following | null>(null);
  /**
   * A press selects what is under it, and wins over an edit still waiting
   * to select its run again (a press beside an open editor commits it, then
   * selects). Every page's read goes stale when the edit lands, so a run
   * pressed is followed instead, by its place and text; anything else drops
   * the follow.
   */
  const select = useCallback(
    (index: number, point: PagePoint) => {
      const next = pick(index, point);
      setSelected(next);
      setFollowing((waiting) => {
        if (!waiting) return waiting;
        if (!next || next.item.kind !== 'run') return null;
        const { rect, text } = next.item;
        // The edited run itself: keep following it where the edit puts it.
        if (next.pageIndex === waiting.pageIndex && sameRect(rect, waiting.expected)) return waiting;
        return { ...waiting, pageIndex: next.pageIndex, fromKey: next.key, expected: rect, text };
      });
    },
    [pick],
  );
  const clear = useCallback(() => {
    setSelected(null);
    setFollowing(null);
  }, []);
  /**
   * Selects again, once page `index` has been edited and read anew, the run
   * now at `expected` showing `text` (`null`: it was removed, select
   * nothing). Call it with the edit, before the step lands.
   */
  const follow = useCallback(
    (index: number, expected: PageRect, text: string | null) => {
      const fromKey = keyAt(index);
      setFollowing(fromKey === undefined ? null : { pageIndex: index, fromKey, expected, text, seenPending: false });
    },
    [keyAt],
  );
  useEffect(() => {
    if (!following) return;
    if (!active) return void setFollowing(null);
    if (paused) {
      if (!following.seenPending) setFollowing({ ...following, seenPending: true });
      return;
    }
    const key = keyAt(following.pageIndex);
    // The edit changed nothing (or never ran): the selection still holds.
    if (key === following.fromKey) {
      if (following.seenPending) setFollowing(null);
      return;
    }
    const state = key === undefined ? undefined : held.get(key);
    if (key !== undefined && (state === undefined || state.status === 'loading')) return;
    setFollowing(null);
    if (key === undefined || state?.status !== 'ready') return;
    const run = followRun(state.content, following.expected, following.text);
    setSelected(run && { pageIndex: following.pageIndex, key, item: run });
  }, [following, active, paused, keyAt, held]);

  // A pick holds only while its page still shows the parse it was found in:
  // an edit, an undo or a move elsewhere makes it name something else.
  const current = (picked: Picked | null) => (picked && keyAt(picked.pageIndex) === picked.key ? picked : null);
  const shownHovered = active ? current(hovered) : null;
  const shownSelected = active ? current(selected) : null;

  const failed = active ? wanted.map(({ index, key }) => ({ index, state: held.get(key) })).find(({ state }) => state?.status === 'failed') : undefined;
  const status = !active
    ? null
    : shownSelected
      ? describeItem({ ...shownSelected.item, pageIndex: shownSelected.pageIndex })
      : failed?.state?.status === 'failed'
        ? `Could not read the content of page ${failed.index + 1}: ${failed.state.message}`
        : CONTENT_HINT;

  return {
    active,
    stateAt,
    hovered: shownHovered,
    selected: shownSelected,
    hover,
    select,
    clear,
    follow,
    /** Whether an edited run is still to be found again (its page not read yet). */
    following: following !== null,
    status,
  };
}

export type ContentMode = ReturnType<typeof useContentMode>;

function sameRect(a: PageRect, b: PageRect): boolean {
  return a.left === b.left && a.top === b.top && a.width === b.width && a.height === b.height;
}

function samePick(a: Picked | null, b: Picked | null): boolean {
  if (!a || !b) return a === b;
  return a.key === b.key && sameItem(a.item, b.item);
}
