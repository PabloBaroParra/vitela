// Ctrl/⌘+P prints the editor's pages while the key is the editor's
// (`scope`: focus in it, or the whole window for a page that is all editor).
// A key aimed elsewhere on a host's page is left to the browser, which
// prints that page. Left alone too while one of the editor's own modal
// dialogs is open (the dialog has the user's attention) and when a handler
// already took the key. `null`: no document, nothing to print.

import { useEffect, useRef } from 'react';
import { WHOLE_DOCUMENT, type EditorScope } from '../document/editorScope';
import { isApplePlatform } from '../document/historyKeys';
import { printKey } from './printText';

export function usePrintKey(onPrint: (() => void) | null, scope: EditorScope = WHOLE_DOCUMENT): void {
  // Read through a ref: a new callback never re-registers the listener.
  const action = useRef(onPrint);
  useEffect(() => {
    action.current = onPrint;
  });
  useEffect(() => {
    const apple = isApplePlatform();
    const listener = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !action.current || !printKey(event, apple)) return;
      if (!scope.ownsEvent(event.target) || scope.modalOpen()) return;
      event.preventDefault();
      action.current();
    };
    window.addEventListener('keydown', listener);
    return () => window.removeEventListener('keydown', listener);
  }, [scope]);
}
