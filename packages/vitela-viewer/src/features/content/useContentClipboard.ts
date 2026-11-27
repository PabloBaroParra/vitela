// Container of the content clipboard (B26 T-262): Copy, Cut and Paste in the
// "Edit content" mode's status row, and Ctrl/⌘ + C, X, V while the mode is
// on — over the group (`useContentGroup`) or the one item selected. The
// clip is the session's (`clipStore`), so it pastes onto any page of any
// document opened after it.
//
// The rules are the core's: what a copy refuses (`copyContentSelection`,
// the text-extraction permission; a cut's own permission first), how a paste
// is written and validated (`pasteContent`, one undo step), where a clip
// came from (its `fromEncrypted`, handed back unchanged). Here, as the native
// shells decide it:
//
// - A cut is the copy for a cut, then the removal as one step: the clip is
//   kept only once the removal is in, as `pdf-ffi`'s `cut_content_selection`
//   hands one back only then.
// - A paste goes where the pointer is over a page (a key press), else to the
//   place the clip was copied from on the first page in view (the button),
//   as the Windows shell does; what it added is selected once the page is
//   read again, so a drag moves the whole paste — still one step.
// - Nothing is asked while a step runs, a copy or cut is still being read
//   (`clipping`: a second cut, or a group delete, would remove the same ids
//   again), or an edited item is still to be found again: ids are valid
//   only for the read they came from. A key pressed meanwhile is the browser's.
// - Keys are the window's while the mode is on and focus is in the editor,
//   never in a text field or one of its dialogs; a copy or cut with nothing
//   selected, and a paste with no clip, are left to the browser.

import { useCallback, useEffect, useRef, useState } from 'react';
import { errorMessage, type ContentItem, type DocumentHandle, type DocumentInfo, type Engine } from '@vitela/core';
import { sentence } from '../../ui/text';
import { pageKey } from '../annotations/useAnnotationEdits';
import { WHOLE_DOCUMENT, type EditorScope } from '../document/editorScope';
import type { ClipStore } from './clipStore';
import type { ContentQueue } from './contentEdits';
import { clipboardAction, membersOf, pastedRect, pastePoint, type PagePointer } from './selection';
import type { ContentGroupState } from './useContentGroup';
import type { ContentMode } from './useContentMode';

const PAGE_CHANGED = 'The page changed before the content could be pasted or cut. Nothing was changed.';

interface ContentClipboardInput {
  readonly engine: Engine;
  readonly handle: DocumentHandle;
  readonly info: Pick<DocumentInfo, 'pages' | 'pageIds'>;
  readonly content: ContentMode;
  readonly group: ContentGroupState;
  readonly queue: ContentQueue;
  readonly store: ClipStore;
  /** Where the pointer is over a page now, or `null`. */
  readonly pointer: () => PagePointer | null;
  /** The positions on screen now, or `null` before the first layout. */
  readonly visible: { readonly first: number; readonly last: number } | null;
  /** Whose keys the editor takes. */
  readonly scope?: EditorScope;
}

/** What is selected now, on one page: the group, or the one item. */
interface Held {
  readonly pageIndex: number;
  readonly items: readonly ContentItem[];
}

const counted = (count: number) => (count === 1 ? '1 item' : `${count} items`);

export function useContentClipboard({ engine, handle, info, content, group, queue, store, pointer, visible, scope = WHOLE_DOCUMENT }: ContentClipboardInput) {
  // Two presses in one render see the same `clipping`: the ref holds the second off.
  const [clipping, setClipping] = useState(false);
  const clippingNow = useRef(false);
  const startClip = useCallback(() => {
    if (clippingNow.current) return false;
    clippingNow.current = true;
    setClipping(true);
    return true;
  }, []);
  const endClip = useCallback(() => {
    clippingNow.current = false;
    setClipping(false);
  }, []);
  const busy = queue.pending || content.following || group.following || clipping;
  const held: Held | null = group.group
    ? { pageIndex: group.group.pageIndex, items: group.group.items }
    : content.selected
      ? { pageIndex: content.selected.pageIndex, items: [content.selected.item] }
      : null;
  const { clip, keep } = store;
  const { notify, apply } = queue;
  const [message, setMessage] = useState<string | null>(null);
  const canCopy = content.active && !!held && !busy;
  const canPaste = content.active && !!clip && !busy;

  const copy = useCallback(() => {
    if (!canCopy || !held || !startClip()) return;
    const { pageIndex, items } = held;
    engine
      .copyContentSelection(handle, pageIndex, membersOf(items))
      .then(
        (copied) => {
          keep(copied);
          setMessage(`Copied ${counted(items.length)}.`);
        },
        (error: unknown) => notify(sentence(errorMessage(error))),
      )
      .finally(endClip);
  }, [canCopy, held, startClip, endClip, engine, handle, keep, notify]);

  const { clear: clearSingle } = content;
  const { clear: clearGroup, followPasted } = group;
  const cut = useCallback(() => {
    if (!canCopy || !held || !startClip()) return;
    const { pageIndex, items } = held;
    const expected = pageKey(info, pageIndex);
    const members = membersOf(items);
    engine
      .copyContentSelection(handle, pageIndex, members, true)
      .then(async (copied) => {
        clearGroup();
        clearSingle();
        const outcome = await apply((session) => {
          if (pageKey(session.info, pageIndex) !== expected) throw new Error(PAGE_CHANGED);
          return { kind: 'removeContentSelection', page: pageIndex, members };
        });
        if (changed(outcome)) {
          keep(copied);
          setMessage(`Cut ${counted(items.length)}.`);
        }
      }, (error: unknown) => notify(sentence(errorMessage(error))))
      // Busy until the removal is in (or refused): a second cut would remove the same ids.
      .finally(endClip);
  }, [canCopy, held, startClip, endClip, info, engine, handle, clearGroup, clearSingle, apply, keep, notify]);

  /** Pastes the clip: at the pointer over a page when `atPointer`, else where it was copied from on the first page in view. */
  const paste = useCallback(
    (atPointer = false) => {
      if (!canPaste || !clip) return;
      const target = pastePoint(clip, atPointer ? pointer() : null, visible?.first ?? 0, info.pages);
      if (!target) return;
      const { pageIndex, at } = target;
      const expected = pageKey(info, pageIndex);
      clearSingle();
      clearGroup();
      followPasted(pageIndex, pastedRect(at, clip, info.pages[pageIndex]?.rotationDegrees ?? 0));
      void apply((session) => {
        if (pageKey(session.info, pageIndex) !== expected) throw new Error(PAGE_CHANGED);
        return { kind: 'pasteContent', page: pageIndex, clip, at };
      }).then((outcome) => changed(outcome) && setMessage(`Pasted on page ${pageIndex + 1}. Drag it into place.`));
    },
    [canPaste, clip, pointer, visible, info, clearSingle, clearGroup, followPasted, apply],
  );

  // Ctrl/⌘ + C, X, V while the mode is on, read through a ref so the listener stays put.
  // Only what may be pressed now is taken: while busy the key is the browser's, not eaten.
  const actions = useRef({ copy, cut, paste, canCopy, canPaste });
  useEffect(() => {
    actions.current = { copy, cut, paste, canCopy, canPaste };
  });
  const { active } = content;
  useEffect(() => {
    if (!active) return;
    const onKey = (event: KeyboardEvent) => {
      if (!scope.ownsEvent(event.target) || scope.modalOpen()) return;
      const action = clipboardAction(event, window.getSelection());
      const now = actions.current;
      if (!action || (action === 'paste' ? !now.canPaste : !now.canCopy)) return;
      event.preventDefault();
      if (action === 'copy') now.copy();
      else if (action === 'cut') now.cut();
      else now.paste(true);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [active, scope]);

  // Leaving the mode drops what it said.
  useEffect(() => {
    if (!active) setMessage(null);
  }, [active]);
  const forget = useCallback(() => setMessage(null), []);

  return {
    /** Whether a copy or cut is still being read: nothing else is asked of the selection meanwhile. */
    clipping,
    canCopy,
    canPaste,
    copy,
    cut,
    paste,
    /** What the last copy, cut or paste said, until the next press on a page; `null` otherwise. */
    message,
    forget,
  };
}

export type ContentClipboard = ReturnType<typeof useContentClipboard>;

function changed(outcome: unknown): boolean {
  return typeof outcome === 'object' && outcome !== null && (outcome as { status?: unknown }).status === 'changed';
}
