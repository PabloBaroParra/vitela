// One editor that holds a draft for the "Edit content" mode's text-block
// edits (B26 T-261): the paragraph editor and the new-line editor share it.
//
// The rules are the run editor's (`useTextRunEditing`, T-259), which keeps
// its own copy:
//
// - The draft lives here, not in the field: the field sits in a virtualized
//   page slot, and a scroll that unmounts the slot commits what was typed
//   (`detach`, a turn later, so a field mounted again at once — React's
//   development double mount — keeps it; `attach` cancels it).
// - Commit and cancel count once: a press beside the field commits it and,
//   in a browser that moves the focus on that press, its blur asks again.
// - The editor is closed a turn later, never inside its own blur handler
//   (the GTK shell hung removing the focused editor from its focus-out).
// - Leaving the mode commits it.

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';

/** An open editor: what it edits (`T`) and what is typed in it now. */
export type Drafted<T> = T & { readonly draft: string };

interface DraftEditorInput<T> {
  /** Whether the mode is on: turning it off commits the open editor. */
  readonly active: boolean;
  /** Records what the editor ends with; the editor is already closing. */
  readonly onCommit: (editor: Drafted<T>) => void;
  /** Gives the keyboard back to the pages. */
  readonly focusPages: () => void;
}

export function useDraftEditor<T extends object>({ active, onCommit, focusPages }: DraftEditorInput<T>) {
  const [editor, setEditor] = useState<Drafted<T> | null>(null);
  /** Bumped on every open and end: a late patch or close for an earlier editor is dropped. */
  const seq = useRef(0);
  const ended = useRef(false);
  // The editor as last rendered, for the callbacks that run outside a render
  // (a save, a blur, an unmount): what is on screen goes in.
  const shown = useRef(editor);
  useLayoutEffect(() => {
    shown.current = editor;
  });
  const commitWith = useRef(onCommit);
  useLayoutEffect(() => {
    commitWith.current = onCommit;
  });

  /** Opens an editor over `target` holding `draft`; answers its ticket, for `patch`. */
  const open = useCallback((target: T, draft: string): number => {
    const ticket = ++seq.current;
    ended.current = false;
    const next = { ...target, draft } as Drafted<T>;
    shown.current = next;
    setEditor(next);
    return ticket;
  }, []);

  /** Updates the editor opened as `ticket` (a style read landing), if it is still open. */
  const patch = useCallback((ticket: number, values: Partial<T>) => {
    setEditor((current) => (current && seq.current === ticket ? { ...current, ...values } : current));
  }, []);

  /** Closes the editor a turn later: never inside the blur that may have committed it. */
  const close = useCallback(() => {
    const ticket = ++seq.current;
    setTimeout(() => {
      if (seq.current === ticket) setEditor(null);
    });
  }, []);

  /** Commits the draft, once, and closes the editor (`refocus`: give the keyboard back to the pages). */
  const commit = useCallback(
    (refocus: boolean) => {
      const open = shown.current;
      if (!open || ended.current) return;
      ended.current = true;
      commitWith.current(open);
      close();
      if (refocus) focusPages();
    },
    [close, focusPages],
  );
  const cancel = useCallback(() => {
    if (!shown.current || ended.current) return;
    ended.current = true;
    close();
    focusPages();
  }, [close, focusPages]);
  /** Each change of the text in the open editor (stable: the field never re-subscribes). */
  const typing = useCallback((text: string) => {
    if (shown.current) shown.current = { ...shown.current, draft: text };
    setEditor((current) => (current ? { ...current, draft: text } : current));
  }, []);
  /** Commits the open editor, if any (before a save, or a press beside it). */
  const commitOpen = useCallback(() => commit(false), [commit]);
  const commitOpenRef = useRef(commitOpen);
  useLayoutEffect(() => {
    commitOpenRef.current = commitOpen;
  });
  useEffect(() => {
    if (!active) commitOpenRef.current();
  }, [active]);

  const detached = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** Whether this hook is still mounted: the viewer going takes the draft with it. */
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const attach = useCallback(() => {
    if (detached.current !== null) clearTimeout(detached.current);
    detached.current = null;
  }, []);
  const detach = useCallback(() => {
    if (detached.current !== null) clearTimeout(detached.current);
    detached.current = setTimeout(() => {
      detached.current = null;
      if (alive.current) commitOpenRef.current();
    });
  }, []);

  return { editor, open, patch, commit, cancel, typing, commitOpen, attach, detach };
}
