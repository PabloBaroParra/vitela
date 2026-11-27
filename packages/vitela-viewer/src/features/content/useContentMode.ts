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

import { useCallback, useEffect, useMemo, useState } from 'react';
import { errorMessage, type ContentItem, type DocumentHandle, type Engine, type PagePoint } from '@vitela/core';
import { sentence } from '../../ui/text';
import {
  CONTENT_HINT,
  contentKey,
  describeItem,
  itemAt,
  keepWanted,
  pagesToParse,
  sameItem,
  type PageContentState,
} from './contentModel';

/** The item under the pointer, or the selected one, with the parse it was found in. */
interface Picked {
  readonly pageIndex: number;
  readonly key: string;
  readonly item: ContentItem;
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
  readonly paintRevision: number;
  readonly baseGeneration: number;
  /** The positions on screen now (no overscan), or `null` before the first layout. */
  readonly visible: { readonly first: number; readonly last: number } | null;
}

export function useContentMode({ engine, handle, active, pageIds, paintRevision, baseGeneration, visible }: ContentModeInput) {
  const keyAt = useCallback(
    (index: number) => {
      const id = pageIds[index];
      return id === undefined ? undefined : contentKey(id, paintRevision, baseGeneration);
    },
    [pageIds, paintRevision, baseGeneration],
  );

  // What to parse: the pages in view and one on each side, by position and key.
  const wanted = useMemo(() => {
    if (!active) return [];
    return pagesToParse(visible, pageIds.length).flatMap((index) => {
      const key = keyAt(index);
      return key === undefined ? [] : [{ index, key }];
    });
  }, [active, visible, pageIds.length, keyAt]);
  const wantedSignature = wanted.map(({ index, key }) => `${index}:${key}`).join(',');

  const [held, setHeld] = useState<ReadonlyMap<string, PageContentState>>(() => new Map());
  useEffect(() => {
    const keys = new Set(wanted.map(({ key }) => key));
    const missing = wanted.filter(({ key }) => !held.has(key));
    // Drop what is no longer wanted, and mark what is about to be parsed.
    if (missing.length > 0 || [...held.keys()].some((key) => !keys.has(key))) {
      setHeld((current) => {
        const next = keepWanted(current, keys);
        for (const { key } of missing) if (!next.has(key)) next.set(key, { status: 'loading' });
        return next;
      });
    }
    for (const { index, key } of missing) {
      // A parse lands only on the key it was asked for, and only while that
      // key is still waiting for it: a page scrolled away or repainted since
      // has dropped it.
      const land = (state: PageContentState) =>
        setHeld((current) => (current.get(key)?.status === 'loading' ? new Map(current).set(key, state) : current));
      engine.readPageContent(handle, index).then(
        (content) => land({ status: 'ready', content }),
        (error: unknown) => land({ status: 'failed', message: sentence(errorMessage(error)) }),
      );
    }
    // `held` is read, not watched: a parse landing must not ask again.
  }, [engine, handle, wantedSignature]);

  const [hovered, setHovered] = useState<Picked | null>(null);
  const [selected, setSelected] = useState<Picked | null>(null);
  // Leaving the mode, or another document, lets both go.
  useEffect(() => {
    if (active) return;
    setHovered(null);
    setSelected(null);
  }, [active]);

  const stateAt = useCallback(
    (index: number): PageContentState | undefined => {
      const key = keyAt(index);
      return key === undefined ? undefined : held.get(key);
    },
    [keyAt, held],
  );

  /** What is under `point` on page `index`, as the parse held now finds it. */
  const pick = useCallback(
    (index: number, point: PagePoint): Picked | null => {
      const key = keyAt(index);
      const state = key === undefined ? undefined : held.get(key);
      if (key === undefined || state?.status !== 'ready') return null;
      const item = itemAt(state.content, point);
      return item && { pageIndex: index, key, item };
    },
    [keyAt, held],
  );

  const hover = useCallback(
    (index: number, point: PagePoint | null) => {
      const next = point ? pick(index, point) : null;
      setHovered((shown) => (samePick(shown, next) ? shown : next));
    },
    [pick],
  );
  const select = useCallback((index: number, point: PagePoint) => setSelected(pick(index, point)), [pick]);
  const clear = useCallback(() => setSelected(null), []);

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
    status,
  };
}

export type ContentMode = ReturnType<typeof useContentMode>;

function samePick(a: Picked | null, b: Picked | null): boolean {
  if (!a || !b) return a === b;
  return a.key === b.key && sameItem(a.item, b.item);
}
