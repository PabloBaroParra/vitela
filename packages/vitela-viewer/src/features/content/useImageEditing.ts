// Container of the "Edit content" mode's image edits (B26 T-260): with an
// image selected, move it (arrow keys, or a drag of its body), resize it (a
// handle), delete it and replace its picture; with none, insert a new one
// where the page is clicked. Each one undo step through the viewer's edit
// queue (`useAnnotationEdits.apply`), which marks the document dirty and that
// a save waits for.
//
// The rules are the core's (`applyEdit` with an `ImageEdit`): the gate, the
// fold of a second move or resize of the same image into its step, the
// checkpoint, the size of an inserted image (`stampPlacement`, asked first
// so the new image can be selected where it lands), whether a picture can be
// replaced. Here: what to ask for and when.
//
// - An edit names the image by its id in the read it was selected in, so
//   none is asked for while another step runs, an edited item is still to
//   be found again or a cut is still being read (`busy`): arrow presses
//   meanwhile add up and go as one move once the image is found again
//   (`ContentMode.followImage`).
// - Each step checks, when its turn comes, that the page at the image's
//   position is still the one it was selected on (`pageKey`).
// - Replace asks the core first (`checkImageReplacement`, once per selection
//   and read), the file picker only after: a refusal is said before the user
//   goes looking for a file, as the native shells do. The picked file is
//   checked like a stamp's (`readStampImage`: PNG or JPEG, `MAX_STAMP_BYTES`).
//   The image is named by its id in the read it was picked in: once the file
//   is read, a step running or the page read anew (another edit landed
//   meanwhile) refuses the replacement instead of naming another image.
// - Insert image picks the file first, then arms the next press on a page
//   (Escape disarms; the pages get the keyboard back), as the Android shell
//   does. A press made while a step runs (one the same press started: a
//   retype it committed) is placed once nothing runs, and keeps the image
//   armed until then.

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { errorMessage, type ContentImage, type DocumentHandle, type DocumentInfo, type Edit, type Engine, type PagePoint, type PageRect } from '@vitela/core';
import { sentence } from '../../ui/text';
import { pageKey } from '../annotations/useAnnotationEdits';
import { readStampImage } from '../annotations/useStamps';
import { useGuardedApply, useHeldMove, type ContentQueue } from './contentEdits';
import { sameRect } from './contentModel';
import type { ContentMode } from './useContentMode';
import { nudgeOf } from './textRuns';

export const INSERT_IMAGE_PROMPT = 'Click the page where the image goes. Press Escape to cancel.';
const PAGE_CHANGED = 'The page changed before the image could be edited. Nothing was changed.';

interface ImageEditingInput {
  readonly engine: Engine;
  readonly handle: DocumentHandle;
  readonly info: Pick<DocumentInfo, 'pages' | 'pageIds'>;
  readonly content: ContentMode;
  readonly queue: ContentQueue;
  /** Gives the keyboard back to the pages (after a toolbar button). */
  readonly focusPages: () => void;
  /** Whether a copy for a cut is still being read: it removes the selection itself once read, so nothing is asked of it meanwhile. */
  readonly clipping?: boolean;
}

/** What the file picker was opened for. */
type Picking =
  | {
      readonly kind: 'replace';
      readonly image: ContentImage;
      readonly pageKey: string;
      /** The read the image's id is valid for (its page's `contentKey` then). */
      readonly contentKey: string;
    }
  | { readonly kind: 'insert' };

/** A press on a page for an armed insert, placed once no step runs. */
interface Placing {
  readonly pageIndex: number;
  readonly point: PagePoint;
  readonly pageKey: string;
}

/** The core's answer for one selected image in one read: `null` replaceable, else its sentence. */
interface Replaceability {
  readonly key: string;
  readonly refusal: string | null;
}

export function useImageEditing({ engine, handle, info, content, queue, focusPages, clipping = false }: ImageEditingInput) {
  const picked = content.selected;
  const image = picked?.item.kind === 'image' ? picked.item : null;
  const pickedKey = picked?.key ?? null;
  const imageKey = picked && image ? `${picked.key}#${image.id}` : null;
  const busy = queue.pending || content.following || clipping;
  const currentPageKey = picked ? pageKey(info, picked.pageIndex) : null;
  const { notify } = queue;
  // Read through a ref: a file read that finishes later checks what holds then.
  const now = useRef({ busy, keyAt: content.keyAt });
  useLayoutEffect(() => {
    now.current = { busy, keyAt: content.keyAt };
  });

  const guarded = useGuardedApply(queue, PAGE_CHANGED);
  const { followImage } = content;
  /** Records `edit` as a step, selecting again the image now at `expected` (`null`: none). */
  const submit = useCallback(
    (pageIndex: number, expectedPageKey: string, edit: Edit, expected: PageRect | null) => {
      followImage(pageIndex, expected);
      guarded(pageIndex, expectedPageKey, edit);
    },
    [followImage, guarded],
  );

  // Arrow presses while busy add up into one move, sent once the image is found again.
  const send = useCallback(
    (dx: number, dy: number) => {
      if (!image || currentPageKey === null || (dx === 0 && dy === 0)) return;
      const { rect } = image;
      submit(image.pageIndex, currentPageKey, { kind: 'moveImage', page: image.pageIndex, imageId: image.id, dx, dy }, {
        ...rect,
        left: rect.left + dx,
        top: rect.top + dy,
      });
    },
    [image, currentPageKey, submit],
  );
  const move = useHeldMove(busy, !!image || content.followingKind === 'image', image, send);

  const resize = useCallback(
    (rect: PageRect) => {
      if (!image || currentPageKey === null || busy || sameRect(rect, image.rect)) return;
      submit(image.pageIndex, currentPageKey, { kind: 'resizeImage', page: image.pageIndex, imageId: image.id, rect }, rect);
    },
    [image, currentPageKey, busy, submit],
  );

  const remove = useCallback(() => {
    if (!image || currentPageKey === null || busy) return;
    submit(image.pageIndex, currentPageKey, { kind: 'removeImage', page: image.pageIndex, imageId: image.id }, null);
  }, [image, currentPageKey, busy, submit]);

  // Whether the selected image can be replaced, asked once nothing runs.
  const [replaceability, setReplaceability] = useState<Replaceability | null>(null);
  useEffect(() => {
    if (!image || !imageKey || busy || replaceability?.key === imageKey) return;
    let live = true;
    engine.checkImageReplacement(handle, image.pageIndex, image.id).then(
      () => live && setReplaceability({ key: imageKey, refusal: null }),
      (error: unknown) => live && setReplaceability({ key: imageKey, refusal: sentence(errorMessage(error)) }),
    );
    return () => {
      live = false;
    };
  }, [engine, handle, image, imageKey, busy, replaceability?.key]);
  const answer = replaceability && replaceability.key === imageKey ? replaceability : null;

  /** What the picker was last opened for: its file goes there. */
  const picking = useRef<Picking | null>(null);
  /** Counts picks: a read that finishes after a later pick began is dropped. */
  const latestPick = useRef(0);

  /** Opens the file picker (`open`) for a replacement, once the core said there can be one. */
  const replace = useCallback(
    (open: () => void) => {
      if (!image || currentPageKey === null || pickedKey === null || busy || !answer) return;
      if (answer.refusal !== null) {
        notify(answer.refusal);
        return;
      }
      picking.current = { kind: 'replace', image, pageKey: currentPageKey, contentKey: pickedKey };
      open();
    },
    [image, currentPageKey, pickedKey, busy, answer, notify],
  );

  // The image an insert places at the next press on a page, and that press.
  const [armed, setArmed] = useState<Uint8Array | null>(null);
  const [placing, setPlacing] = useState<Placing | null>(null);
  const insert = useCallback((open: () => void) => {
    picking.current = { kind: 'insert' };
    open();
  }, []);
  const disarm = useCallback(() => {
    setArmed(null);
    setPlacing(null);
  }, []);
  // Leaving the mode disarms.
  useEffect(() => {
    if (!content.active) disarm();
  }, [content.active, disarm]);

  /** The file the picker came back with. */
  const filePicked = useCallback(
    async (file: File) => {
      const purpose = picking.current;
      picking.current = null;
      if (!purpose) return;
      const ticket = ++latestPick.current;
      const result = await readStampImage(file);
      if (ticket !== latestPick.current) return;
      if ('refusal' in result) {
        notify(result.refusal);
        return;
      }
      const { bytes } = result.image;
      if (purpose.kind === 'insert') {
        setArmed(bytes);
        // The Insert button keeps the focus otherwise, and Escape is the pages'.
        focusPages();
        return;
      }
      const target = purpose.image;
      // The id names the image only in the read it was picked in.
      if (now.current.busy || now.current.keyAt(target.pageIndex) !== purpose.contentKey) {
        notify(PAGE_CHANGED);
        return;
      }
      submit(target.pageIndex, purpose.pageKey, { kind: 'replaceImage', page: target.pageIndex, imageId: target.id, bytes }, target.rect);
      focusPages();
    },
    [notify, submit, focusPages],
  );

  /**
   * A press on page `pageIndex` while an insert is armed: the image goes
   * there, once no step runs. `false` when none is armed.
   */
  const place = useCallback(
    (pageIndex: number, point: PagePoint): boolean => {
      if (!armed) return false;
      setPlacing({ pageIndex, point, pageKey: pageKey(info, pageIndex) });
      return true;
    },
    [armed, info],
  );
  // Never while a step runs: a press that committed a retype first started
  // one, and the press's own render does not know it yet.
  useEffect(() => {
    if (!placing || !armed || busy) return;
    const bytes = armed;
    const { pageIndex, point, pageKey: expectedPageKey } = placing;
    setArmed(null);
    setPlacing(null);
    // The core's size, asked first: the new image is selected where it lands.
    engine.stampPlacement(handle, pageIndex, bytes, point).then(
      (rect) => submit(pageIndex, expectedPageKey, { kind: 'insertImage', page: pageIndex, at: point, bytes }, rect),
      (error: unknown) => notify(sentence(errorMessage(error))),
    );
  }, [placing, armed, busy, engine, handle, submit, notify]);

  /** The keys the pages take for the image edits; `true` when one was used. */
  const onKey = useCallback(
    (key: string, shift: boolean): boolean => {
      if (armed && key === 'Escape') {
        disarm();
        return true;
      }
      if (!image) {
        // Only an arrow, and only while the image it moves is being found again.
        const nudge = content.followingKind === 'image' ? nudgeOf(key, shift) : null;
        if (nudge) move(nudge.dx, nudge.dy);
        return nudge !== null;
      }
      if (key === 'Delete' || key === 'Backspace') {
        remove();
        return true;
      }
      const nudge = nudgeOf(key, shift);
      if (!nudge) return false;
      move(nudge.dx, nudge.dy);
      return true;
    },
    [armed, disarm, image, content.followingKind, move, remove],
  );

  return {
    image,
    busy,
    /** Whether the core said the selected image can be replaced. */
    replaceable: !!answer && answer.refusal === null,
    /** Why it cannot, as the core's sentence; `null` when it can or is still being asked. */
    replaceRefusal: answer?.refusal ?? null,
    move,
    resize,
    remove,
    replace,
    insert,
    picked: filePicked,
    /** Whether an insert waits for a press on a page. */
    armed: armed !== null,
    /** What the status row says while armed; `null` otherwise. */
    prompt: armed ? INSERT_IMAGE_PROMPT : null,
    place,
    disarm,
    onKey,
  };
}

export type ImageEditing = ReturnType<typeof useImageEditing>;
