// Copying the selected text: the Copy button and Ctrl/⌘+C put the core's
// text for the selection on the clipboard. Only ever handed text the engine
// read for a selection, so a document that withholds its text has nothing
// to copy. Container logic only, no markup.

import { useCallback, useEffect, useState } from 'react';

export type CopyStatus = 'idle' | 'copied' | 'failed';

/**
 * Whether `event` is a plain Ctrl/⌘+C this viewer should answer: not one
 * typed into a field, and not one meant for text the browser itself has
 * selected on the page (a status line, say).
 */
export function isCopyShortcut(event: KeyboardEvent, domSelection: Selection | null): boolean {
  if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return false;
  if (event.key.toLowerCase() !== 'c') return false;
  if (domSelection && !domSelection.isCollapsed) return false;
  const target = event.target;
  if (target instanceof HTMLElement) {
    if (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) return false;
  }
  return true;
}

/** `text` is the selection to copy; `null` when nothing is selected. */
export function useCopy(text: string | null) {
  // Tied to the text it is about: a new selection starts from idle.
  const [result, setResult] = useState<{ text: string; status: CopyStatus } | null>(null);
  const status: CopyStatus = result && result.text === text ? result.status : 'idle';

  const copy = useCallback(() => {
    if (!text) return;
    // Called straight from the click or key press, so the browser counts it
    // as a user gesture and lets it write.
    navigator.clipboard.writeText(text).then(
      () => setResult({ text, status: 'copied' }),
      () => setResult({ text, status: 'failed' }),
    );
  }, [text]);

  useEffect(() => {
    if (!text) return;
    const onKey = (event: KeyboardEvent) => {
      if (!isCopyShortcut(event, window.getSelection())) return;
      event.preventDefault();
      copy();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [text, copy]);

  return { copy, status };
}
