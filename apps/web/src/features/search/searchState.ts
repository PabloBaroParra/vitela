// What a search shows and how moving between its matches works. Pure; the
// search itself is the core's (exact, case-sensitive) — nothing here
// matches text.

import type { PageRects, SearchHit } from '../../core/types';

export type SearchState =
  | { readonly kind: 'idle' }
  | { readonly kind: 'searching'; readonly query: string }
  /**
   * `current` indexes `hits` (0 with no hits). Every move between matches
   * makes a new state object, even onto the same match, so the viewer can
   * scroll to it again.
   */
  | { readonly kind: 'found'; readonly query: string; readonly hits: readonly SearchHit[]; readonly current: number }
  | { readonly kind: 'failed'; readonly query: string; readonly message: string };

export const IDLE: SearchState = { kind: 'idle' };

/** Moves `delta` matches on, wrapping at either end. Other states stay put. */
export function step(state: SearchState, delta: number): SearchState {
  if (state.kind !== 'found' || state.hits.length === 0) return state;
  const count = state.hits.length;
  return { ...state, current: (((state.current + delta) % count) + count) % count };
}

/** The match the viewer shows as current, if any. */
export function currentHit(state: SearchState): SearchHit | null {
  return state.kind === 'found' ? (state.hits[state.current] ?? null) : null;
}

/** The matches of a finished search; none otherwise. */
export function hitsOf(state: SearchState): readonly SearchHit[] {
  return state.kind === 'found' ? state.hits : NO_HITS;
}

const NO_HITS: readonly SearchHit[] = [];

/**
 * Each page's matches, for the page's text layer. Build it once per search
 * (`hitsOf` keeps one array per search), so a page keeps the same array, and
 * skips re-rendering, while only the current match moves.
 */
export function hitsByPage(hits: readonly SearchHit[]): ReadonlyMap<number, readonly PageRects[]> {
  const pages = new Map<number, PageRects[]>();
  for (const hit of hits) {
    const onPage = pages.get(hit.pageIndex);
    if (onPage) onPage.push(hit.rects);
    else pages.set(hit.pageIndex, [hit.rects]);
  }
  return pages;
}

/** The result line next to the search box. */
export function summary(state: SearchState): string {
  switch (state.kind) {
    case 'idle':
      return '';
    case 'searching':
      return 'Searching…';
    case 'found':
      return state.hits.length === 0 ? 'No matches' : `${state.current + 1} of ${state.hits.length}`;
    case 'failed':
      return `Search failed: ${state.message}`;
  }
}
