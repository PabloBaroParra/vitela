// Undo and redo from the keyboard while the viewer is on screen, with focus
// in the editor (`scope`). Which keys, and when they are left alone (text
// fields, the editor's modal dialogs, keys already taken), is
// `historyAction`'s call — the same one Organize makes.

import { useEffect, useRef } from 'react';
import { WHOLE_DOCUMENT, type EditorScope } from '../document/editorScope';
import { historyAction, isApplePlatform } from '../document/historyKeys';

export function useUndoKeys(onUndo: () => void, onRedo: () => void, scope: EditorScope = WHOLE_DOCUMENT): void {
  // Read through a ref: new callbacks never re-register the listener.
  const actions = useRef({ onUndo, onRedo });
  useEffect(() => {
    actions.current = { onUndo, onRedo };
  });
  useEffect(() => {
    const apple = isApplePlatform();
    const listener = (event: KeyboardEvent) => {
      if (!scope.ownsEvent(event.target)) return;
      const action = historyAction(event, apple, scope);
      if (!action) return;
      event.preventDefault();
      if (action === 'undo') actions.current.onUndo();
      else actions.current.onRedo();
    };
    window.addEventListener('keydown', listener);
    return () => window.removeEventListener('keydown', listener);
  }, [scope]);
}
