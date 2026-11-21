// Undo and redo from the keyboard while the viewer is on screen, from
// anywhere on the page. Which keys, and when they are left alone (text
// fields, modal dialogs, keys already taken), is `historyAction`'s call —
// the same one Organize makes.

import { useEffect, useRef } from 'react';
import { historyAction, isApplePlatform } from '../document/historyKeys';

export function useUndoKeys(onUndo: () => void, onRedo: () => void): void {
  // Read through a ref: new callbacks never re-register the listener.
  const actions = useRef({ onUndo, onRedo });
  useEffect(() => {
    actions.current = { onUndo, onRedo };
  });
  useEffect(() => {
    const apple = isApplePlatform();
    const listener = (event: KeyboardEvent) => {
      const action = historyAction(event, apple);
      if (!action) return;
      event.preventDefault();
      if (action === 'undo') actions.current.onUndo();
      else actions.current.onRedo();
    };
    window.addEventListener('keydown', listener);
    return () => window.removeEventListener('keydown', listener);
  }, []);
}
