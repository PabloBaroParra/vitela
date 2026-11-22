// How an editing screen tells the save state "the document changed". The
// workspace owns the save state (one per open document) and provides
// `markDirty` here, so the screens that edit — Organize (T-226) and later —
// reach it without the save controls having to sit above them.

import { createContext, useContext } from 'react';

export const MarkDirtyContext = createContext<(() => void) | null>(null);

/** Call the result after each edit that was applied to the open document. */
export function useMarkDirty(): () => void {
  const markDirty = useContext(MarkDirtyContext);
  // Failing loudly: a screen outside the provider would never flag its edits.
  if (!markDirty) throw new Error('useMarkDirty needs the workspace that owns the save state');
  return markDirty;
}
