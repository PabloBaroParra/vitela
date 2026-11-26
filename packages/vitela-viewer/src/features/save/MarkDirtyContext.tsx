// How an editing screen tells the save state "the document changed". The
// workspace owns the save state (one per open document) and provides
// `markDirty` here, so the screens that edit — Organize (T-226) and later —
// reach it without the save controls having to sit above them.

import { createContext, useContext, useEffect } from 'react';

export const MarkDirtyContext = createContext<(() => void) | null>(null);

/** Call the result after each edit that was applied to the open document. */
export function useMarkDirty(): () => void {
  const markDirty = useContext(MarkDirtyContext);
  // Failing loudly: a screen outside the provider would never flag its edits.
  if (!markDirty) throw new Error('useMarkDirty needs the workspace that owns the save state');
  return markDirty;
}

/**
 * How an editing screen hands its edit queue to the save state, so a save
 * waits for the edits asked for before it (see `useSave`'s `waitForEdits`).
 */
export const SaveWaitsForEditsContext = createContext<((settled: () => Promise<unknown>) => () => void) | null>(null);

/** Makes each save wait for `settled` while the screen is mounted (nothing to wait for outside the workspace). */
export function useSaveWaitsFor(settled: () => Promise<unknown>): void {
  const waitForEdits = useContext(SaveWaitsForEditsContext);
  useEffect(() => waitForEdits?.(settled), [waitForEdits, settled]);
}
