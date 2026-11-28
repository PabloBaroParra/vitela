// A press of the "Edit content" mode that landed on a page whose content was
// still being read: it picked nothing then, so it is held — one at most, the
// newest — and resolved against that read once it lands. Only a plain press
// waits (a selection, never a drag); a later press, Escape, leaving the mode
// or another page at its position lets it go, and so does a failed read.

import type { PageContent, PagePoint } from '@vitela/core';
import type { PageContentState } from './contentModel';

/** A press waiting for its page's read: where it was, and which read it waits for. */
export interface PendingPress {
  readonly pageIndex: number;
  /** The `contentKey` its page had when it was pressed. */
  readonly key: string;
  readonly point: PagePoint;
}

/** What to do with a pending press now. */
export type PressOutcome = { readonly kind: 'wait' } | { readonly kind: 'drop' } | { readonly kind: 'resolve'; readonly content: PageContent };

/** Whether a press on a page in `state` must wait for its read: none yet, or one still running. */
export function pressWaits(state: PageContentState | undefined): boolean {
  return state === undefined || state.status === 'loading';
}

/**
 * Where `press` stands, with its position showing `keyNow` in `state`, and
 * `wanted` whether the mode still reads that page.
 */
export function settlePress(press: PendingPress, keyNow: string | undefined, state: PageContentState | undefined, wanted: boolean): PressOutcome {
  if (keyNow !== press.key || !wanted) return { kind: 'drop' };
  if (pressWaits(state)) return { kind: 'wait' };
  return state?.status === 'ready' ? { kind: 'resolve', content: state.content } : { kind: 'drop' };
}
