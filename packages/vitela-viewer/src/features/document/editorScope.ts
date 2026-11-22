// Which page events are the editor's. Embedded in a page it shares with a
// host site (`'editor'`, the default), a key is the editor's only while focus
// is in it and a dropped file only over it; on a page that is all editor
// (`'window'`, the app) every key and drop on the window is. Either way, a
// modal dialog stops the editor's keys only when it is one of the editor's own.
//
// "Focus in the editor": a key aimed at an element inside the root. A click
// on the page area focuses nothing, a focused control that goes away leaves
// focus nowhere, and a dropped file moves no focus at all; in each case the
// key is aimed at <body>. Such a key is the editor's when the user's last
// press, focus move or drop was inside it.

import { createContext, useEffect, useRef, useState, type RefObject } from 'react';

/** Which events the editor takes: only its own (focus or pointer in it), or every one on the window. */
export type EventScope = 'editor' | 'window';

export interface EditorScope {
  /** Whether a key event aimed at `target` is the editor's. */
  readonly ownsEvent: (target: EventTarget | null) => boolean;
  /** Whether a file dragged over `target` is the editor's to take. */
  readonly takesDrop: (target: EventTarget | null) => boolean;
  /** Whether one of the editor's modal dialogs is open (the editor opens dialogs only as modals). */
  readonly modalOpen: () => boolean;
}

/** Whether a modal dialog is open inside `within`. */
export function modalOpenIn(within: ParentNode | null | undefined): boolean {
  return !!within?.querySelector('dialog[open], [aria-modal="true"]');
}

/** The whole page is the editor's: the scope of a hook used outside a workspace. */
export const WHOLE_DOCUMENT: EditorScope = {
  ownsEvent: () => true,
  takesDrop: () => true,
  modalOpen: () => modalOpenIn(globalThis.document),
};

/** Provided by the workspace to the screens inside it. */
export const EditorScopeContext = createContext<EditorScope>(WHOLE_DOCUMENT);

const contains = (root: Element | null, target: EventTarget | null) => !!root && target instanceof Node && root.contains(target);

/** Aimed at nothing in particular: what a key goes to while nothing has focus. */
function unfocused(target: EventTarget | null): boolean {
  const document = globalThis.document;
  return target === document || target === document.body || target === document.documentElement;
}

/** The user acting somewhere on the page: what decides where an unfocused key belongs. */
const INTERACTIONS = ['pointerdown', 'focusin', 'drop'] as const;

/**
 * The scope of the editor whose root element gets `rootRef`. The scope is
 * one stable object; it reads the root, the mode and the last interaction
 * when asked.
 */
export function useEditorScope(eventScope: EventScope = 'editor'): {
  readonly rootRef: RefObject<HTMLDivElement | null>;
  readonly scope: EditorScope;
} {
  const rootRef = useRef<HTMLDivElement | null>(null);
  /** Whether the user's last press, focus move or drop landed inside the root. */
  const lastInside = useRef(false);
  const mode = useRef(eventScope);
  useEffect(() => {
    mode.current = eventScope;
  }, [eventScope]);

  useEffect(() => {
    // Captured on the window: seen before anything inside can stop it.
    const note = (event: Event) => {
      lastInside.current = contains(rootRef.current, event.target);
    };
    for (const type of INTERACTIONS) window.addEventListener(type, note, true);
    return () => {
      for (const type of INTERACTIONS) window.removeEventListener(type, note, true);
    };
  }, []);

  const [scope] = useState<EditorScope>(() => ({
    ownsEvent: (target) =>
      mode.current === 'window' || contains(rootRef.current, target) || (unfocused(target) && lastInside.current),
    takesDrop: (target) => mode.current === 'window' || contains(rootRef.current, target),
    modalOpen: () => modalOpenIn(rootRef.current),
  }));
  return { rootRef, scope };
}
