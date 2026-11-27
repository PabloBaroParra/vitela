// The browser's "Leave site? Changes you made may not be saved." warning
// (T-267): asked for while the open document has unsaved edits, and only
// then. A `beforeunload` listener keeps a page out of the back/forward cache
// in some browsers, so it is removed as soon as the document is clean again.
//
// Driven by the workspace's `onDirtyChange`, as the PWA update's hold is: the
// update's Reload refuses while the document is dirty, so the reload it does
// happens with no listener here and never asks.

type UnloadTarget = Pick<Window, 'addEventListener' | 'removeEventListener'>;

function warn(event: BeforeUnloadEvent) {
  event.preventDefault();
  // Browsers from before preventDefault asked only when this was set.
  event.returnValue = true;
}

/** Returns what to tell whether the open document has unsaved edits. */
export function unloadWarning(target: UnloadTarget): (dirty: boolean) => void {
  let listening = false;
  return (dirty) => {
    if (dirty === listening) return;
    listening = dirty;
    if (dirty) target.addEventListener('beforeunload', warn);
    else target.removeEventListener('beforeunload', warn);
  };
}
