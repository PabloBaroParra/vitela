// Container of the "Edit content" mode's text-run edits (B26 T-259): with a
// run selected, retype it in a camouflaged editor over it, move it (arrow
// keys, or a drag of its outline), delete it, and turn bold, italic and
// underline on or off — each one undo step through the viewer's edit queue
// (`useAnnotationEdits.apply`), which marks the document dirty and that a
// save waits for.
//
// The rules are the core's (`applyEdit` with a `TextRunEdit`): the gate,
// the fold of a second retype or move of the same run into its step, the
// checkpoint, the bold sibling. Here: what to ask for and when.
//
// - An edit names the run by its id in the read it was selected in, so none
//   is asked for while another step runs, the page has not been read again
//   since the last one or a cut is still being read (`busy`): arrow presses
//   meanwhile add up and go as one move once the run is found again
//   (`ContentMode.follow`).
// - Each step checks, when its turn comes, that the page at the run's
//   position is still the one it was selected on (`pageKey`).
// - The editor commits on Enter and when it loses focus, and cancels on
//   Escape; it is closed a turn later, never inside its own blur handler (the
//   GTK shell hung removing the focused editor from its focus-out). A save,
//   leaving the mode, another selection, or its page scrolling out of the
//   mounted range commits it first: the draft is held here, not in the field.

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { ContentRun, DocumentHandle, DocumentInfo, Edit, Engine, TextRunLook, TextRunLookState, TextRunStyle } from '@vitela/core';
import { pageKey } from '../annotations/useAnnotationEdits';
import { useGuardedApply, useHeldMove, type ContentQueue } from './contentEdits';
import type { ContentMode } from './useContentMode';
import { createRunFontRegistry, type RunFontRegistry } from './runFont';
import { nudgeOf, retypeOf, toggled, type FaceToggle } from './textRuns';

/** The editor open over a run: the run, where it was selected, and how to draw it. */
export interface OpenEditor {
  readonly run: ContentRun;
  readonly pageKey: string;
  readonly style: TextRunStyle | null;
  /** The look the core reads for the run, a queued restyle included; `null` when not read yet. */
  readonly look: TextRunLook | null;
  /** The family the run's embedded program loaded as; `null` until it does, or when it cannot. */
  readonly family: string | null;
  /** What is typed in it now. */
  readonly draft: string;
}

const PAGE_CHANGED = 'The page changed before the text could be edited. Nothing was changed.';

interface TextRunEditingInput {
  readonly engine: Engine;
  readonly handle: DocumentHandle;
  readonly info: Pick<DocumentInfo, 'pages' | 'pageIds'>;
  readonly content: ContentMode;
  readonly queue: ContentQueue;
  /** Gives the keyboard back to the pages (after Enter in the editor). */
  readonly focusPages: () => void;
  /** Whether a copy for a cut is still being read: it removes the selection itself once read, so nothing is asked of it meanwhile. */
  readonly clipping?: boolean;
}

export function useTextRunEditing({ engine, handle, info, content, queue, focusPages, clipping = false }: TextRunEditingInput) {
  const picked = content.selected;
  const run = picked?.item.kind === 'run' ? picked.item : null;
  const runKey = picked && run ? `${picked.key}#${run.id}` : null;
  const busy = queue.pending || content.following || clipping;

  // The look the toggles show, read for the selected run once nothing runs.
  const [look, setLook] = useState<{ readonly key: string; readonly state: TextRunLookState | null } | null>(null);
  useEffect(() => {
    if (!run || !runKey || busy || look?.key === runKey) return;
    let live = true;
    engine.textRunLook(handle, run.pageIndex, run.id).then(
      (state) => live && setLook({ key: runKey, state }),
      () => live && setLook({ key: runKey, state: null }),
    );
    return () => {
      live = false;
    };
  }, [engine, handle, run, runKey, busy, look?.key]);
  const shownLook = look && look.key === runKey ? look.state : null;

  const guarded = useGuardedApply(queue, PAGE_CHANGED);
  const { follow } = content;
  /** Records `edit` as a step for `target`, selecting again the run now at `expected` showing `text`. */
  const submit = useCallback(
    (target: ContentRun, expectedPageKey: string, edit: Edit, text: string | null, dx = 0, dy = 0) => {
      const { rect } = target;
      follow(target.pageIndex, { ...rect, left: rect.left + dx, top: rect.top + dy }, text);
      guarded(target.pageIndex, expectedPageKey, edit);
    },
    [follow, guarded],
  );
  const currentPageKey = picked ? pageKey(info, picked.pageIndex) : null;

  // Arrow presses while busy add up into one move, sent once the run is found again.
  const send = useCallback(
    (dx: number, dy: number) => {
      if (!run || currentPageKey === null) return;
      submit(run, currentPageKey, { kind: 'moveTextRun', page: run.pageIndex, runId: run.id, dx, dy }, run.text, dx, dy);
    },
    [run, currentPageKey, submit],
  );
  const move = useHeldMove(busy, !!run || content.followingKind === 'run', run, send);

  const remove = useCallback(() => {
    if (!run || currentPageKey === null || busy) return;
    submit(run, currentPageKey, { kind: 'removeTextRun', page: run.pageIndex, runId: run.id }, null);
  }, [run, currentPageKey, busy, submit]);

  const toggle = useCallback(
    (face: FaceToggle) => {
      if (!run || currentPageKey === null || busy || !shownLook) return;
      const { bold, italic, underline, strike } = shownLook;
      const to = toggled({ bold, italic, underline, strike }, face);
      submit(run, currentPageKey, { kind: 'restyleTextRuns', page: run.pageIndex, runIds: [run.id], look: to }, run.text);
    },
    [run, currentPageKey, busy, shownLook, submit],
  );

  // The faces the editors load, for this document only: taken off the page
  // when it closes or another opens.
  const fonts = useRef<RunFontRegistry | null>(null);
  useEffect(() => {
    const registry = createRunFontRegistry(engine, handle);
    fonts.current = registry;
    return () => {
      registry.dispose();
      if (fonts.current === registry) fonts.current = null;
    };
  }, [engine, handle]);

  // The editor over the selected run. What is typed in it is held here, not
  // in the field: the field lives in a virtualized page slot, and a scroll
  // that unmounts the slot must not lose it (`detach` commits it).
  const [editor, setEditor] = useState<OpenEditor | null>(null);
  const openSeq = useRef(0);
  /**
   * Whether the open editor was already ended — the one owner of "commit
   * once": a press beside it commits it and, in a browser that moves the
   * focus on that press, its blur then asks again before it is closed;
   * Enter commits and the blur its closing causes asks again. Only the
   * first counts.
   */
  const ended = useRef(false);
  const open = useCallback(() => {
    if (!run || currentPageKey === null || busy || editor) return;
    if (!run.editable) {
      queue.notify('This text cannot be retyped: its font does not say what all of its characters are.');
      return;
    }
    const seq = ++openSeq.current;
    ended.current = false;
    const target = run;
    // A queued restyle shows in the look the toggles read, not in the file's style.
    setEditor({ run: target, pageKey: currentPageKey, style: null, look: shownLook, family: null, draft: target.text });
    const update = (patch: Partial<OpenEditor>) =>
      setEditor((shown) => (shown && openSeq.current === seq ? { ...shown, ...patch } : shown));
    engine.textRunStyle(handle, target.pageIndex, target.id).then(
      (style) => update({ style }),
      () => {},
    );
    const pageId = info.pageIds[target.pageIndex];
    if (pageId !== undefined) {
      const at = { pageIndex: target.pageIndex, runId: target.id, pageId, fontName: target.fontName, baseGeneration: content.baseGeneration };
      void fonts.current?.load(at).then((family) => family && update({ family }));
    }
  }, [run, currentPageKey, busy, editor, queue, engine, handle, shownLook, info.pageIds, content.baseGeneration]);

  // An editor opened before the look read landed takes it when it does:
  // the camouflage shows the run's queued restyle, not the file's face.
  const editorRunKey = editor && picked && editor.run === run ? runKey : null;
  useEffect(() => {
    if (!editorRunKey || !shownLook || editorRunKey !== runKey) return;
    setEditor((shown) => (shown && shown.look === null ? { ...shown, look: shownLook } : shown));
  }, [editorRunKey, runKey, shownLook]);

  /** Closes the editor a turn later: never inside the blur that may have committed it. */
  const close = useCallback(() => {
    const seq = openSeq.current;
    setTimeout(() => {
      if (openSeq.current === seq) setEditor(null);
    });
  }, []);

  // The editor as last rendered, for the callbacks that run outside a render
  // (a save, a blur, an unmount): what is on screen goes in.
  const shownEditor = useRef(editor);
  useLayoutEffect(() => {
    shownEditor.current = editor;
  });
  /** Commits the draft of the open editor, once, and closes it (`refocus`: give the keyboard back to the pages). */
  const commit = useCallback(
    (refocus: boolean) => {
      const open = shownEditor.current;
      if (!open || ended.current) return;
      ended.current = true;
      const edit = retypeOf(open.run, open.draft);
      if (edit) submit(open.run, open.pageKey, edit, edit.kind === 'removeTextRun' ? null : open.draft);
      openSeq.current += 1;
      close();
      if (refocus) focusPages();
    },
    [submit, close, focusPages],
  );
  const commitRef = useRef(commit);
  useLayoutEffect(() => {
    commitRef.current = commit;
  });
  const cancel = useCallback(() => {
    if (ended.current) return;
    ended.current = true;
    openSeq.current += 1;
    close();
    focusPages();
  }, [close, focusPages]);
  /** Each change of the text in the open editor (stable: the field never re-subscribes). */
  const typing = useCallback((text: string) => {
    if (shownEditor.current) shownEditor.current = { ...shownEditor.current, draft: text };
    setEditor((shown) => (shown ? { ...shown, draft: text } : shown));
  }, []);
  /** Commits the open editor, if any (before a save, or a press beside it). */
  const commitOpen = useCallback(() => commitRef.current(false), []);
  // Leaving the mode commits what was typed.
  useEffect(() => {
    if (!content.active) commitOpen();
  }, [content.active, commitOpen]);

  // The field went (its page slot scrolled out of the overscan): commit its
  // draft, as leaving it would — a turn later, so a field mounted again at
  // once (React's development double mount) keeps it, and never inside the
  // unmount of the focused element.
  const detached = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** Whether this hook is still mounted: the viewer going takes the draft with it. */
  const alive = useRef(true);
  const attach = useCallback(() => {
    if (detached.current !== null) clearTimeout(detached.current);
    detached.current = null;
  }, []);
  const detach = useCallback(() => {
    if (detached.current !== null) clearTimeout(detached.current);
    detached.current = setTimeout(() => {
      detached.current = null;
      if (alive.current) commitRef.current(false);
    });
  }, []);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  /** The keys the pages take for the selected run; `true` when one was used. */
  const onKey = useCallback(
    (key: string, shift: boolean): boolean => {
      if (editor) return false;
      if (!run) {
        // Only an arrow, and only while the run it moves is being found again.
        const nudge = content.followingKind === 'run' ? nudgeOf(key, shift) : null;
        if (nudge) move(nudge.dx, nudge.dy);
        return nudge !== null;
      }
      if (key === 'Enter') {
        open();
        return true;
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
    [run, editor, content.followingKind, open, remove, move],
  );

  return {
    run,
    look: shownLook,
    busy,
    editor,
    open,
    commit,
    cancel,
    typing,
    commitOpen,
    attach,
    detach,
    move,
    remove,
    toggle,
    onKey,
  };
}

export type TextRunEditing = ReturnType<typeof useTextRunEditing>;
