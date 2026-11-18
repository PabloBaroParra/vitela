// Undo and redo from the keyboard while the viewer is on screen: Ctrl/⌘+Z,
// Ctrl/⌘+Shift+Z and Ctrl/⌘+Y, from anywhere on the page — but never taken
// from a text field (the search box keeps its own undo) or from a handler
// that already took the key.

import { useEffect, useRef } from 'react';

export type HistoryKey = 'undo' | 'redo';

/** What a key press asks of the history, if anything. */
export function historyKey(event: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey'>): HistoryKey | null {
  if (!(event.ctrlKey || event.metaKey) || event.altKey) return null;
  const key = event.key.toLowerCase();
  if (key === 'z') return event.shiftKey ? 'redo' : 'undo';
  if (key === 'y' && !event.shiftKey) return 'redo';
  return null;
}

function isTextField(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return !!target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])');
}

export function useUndoKeys(onUndo: () => void, onRedo: () => void): void {
  // Read through a ref: new callbacks never re-register the listener.
  const actions = useRef({ onUndo, onRedo });
  useEffect(() => {
    actions.current = { onUndo, onRedo };
  });
  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (event.defaultPrevented || isTextField(event.target)) return;
      const action = historyKey(event);
      if (!action) return;
      event.preventDefault();
      if (action === 'undo') actions.current.onUndo();
      else actions.current.onRedo();
    };
    window.addEventListener('keydown', listener);
    return () => window.removeEventListener('keydown', listener);
  }, []);
}
