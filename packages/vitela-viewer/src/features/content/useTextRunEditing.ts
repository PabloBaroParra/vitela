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
//   is asked for while another step runs or the page has not been read again
//   since the last one (`busy`): arrow presses meanwhile add up and go as one
//   move once the run is found again (`ContentMode.follow`).
// - Each step checks, when its turn comes, that the page at the run's
//   position is still the one it was selected on (`pageKey`).
// - The editor commits on Enter and when it loses focus, and cancels on
//   Escape; it is closed a turn later, never inside its own blur handler (the
//   GTK shell hung removing the focused editor from its focus-out). A save,
//   leaving the mode or another selection commits it first.

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { ContentRun, DocumentHandle, DocumentInfo, Edit, Engine, TextRunLookState, TextRunStyle } from '@vitela/core';
import { pageKey, type Session } from '../annotations/useAnnotationEdits';
import type { ContentMode } from './useContentMode';
import { loadRunFont } from './runFont';
import { nudgeOf, retypeOf, toggled, type FaceToggle } from './textRuns';

/** What the text-run edits need of the viewer's edit queue. */
export interface TextRunQueue {
  readonly pending: boolean;
  readonly apply: (build: (session: Session) => Edit | null) => Promise<unknown>;
  readonly notify: (message: string) => void;
}

/** The editor open over a run: the run, where it was selected, and how to draw it. */
export interface OpenEditor {
  readonly run: ContentRun;
  readonly pageKey: string;
  readonly style: TextRunStyle | null;
  /** The family the run's embedded program loaded as; `null` until it does, or when it cannot. */
  readonly family: string | null;
}

const PAGE_CHANGED = 'The page changed before the text could be edited. Nothing was changed.';

interface TextRunEditingInput {
  readonly engine: Engine;
  readonly handle: DocumentHandle;
  readonly info: Pick<DocumentInfo, 'pages' | 'pageIds'>;
  readonly baseGeneration: number;
  readonly content: ContentMode;
  readonly queue: TextRunQueue;
  /** Gives the keyboard back to the pages (after Enter in the editor). */
  readonly focusPages: () => void;
}

export function useTextRunEditing({ engine, handle, info, baseGeneration, content, queue, focusPages }: TextRunEditingInput) {
  const picked = content.selected;
  const run = picked?.item.kind === 'run' ? picked.item : null;
  const runKey = picked && run ? `${picked.key}#${run.id}` : null;
  const busy = queue.pending || content.following;

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

  /** Records `edit` as a step for `target`, selecting again the run now at `expected` showing `text`. */
  const submit = useCallback(
    (target: ContentRun, expectedPageKey: string, edit: Edit, text: string | null, dx = 0, dy = 0) => {
      const { rect } = target;
      content.follow(target.pageIndex, { ...rect, left: rect.left + dx, top: rect.top + dy }, text);
      void queue.apply((session) => {
        if (pageKey(session.info, target.pageIndex) !== expectedPageKey) throw new Error(PAGE_CHANGED);
        return edit;
      });
    },
    [content, queue],
  );
  const currentPageKey = picked ? pageKey(info, picked.pageIndex) : null;

  // Arrow presses while busy add up into one move, sent once the run is found again.
  const nudged = useRef({ dx: 0, dy: 0 });
  const move = useCallback(
    (dx: number, dy: number) => {
      // While the moved run is found again its old pick no longer holds, but
      // the presses meanwhile are still for it.
      if (busy && (run || content.following)) {
        nudged.current = { dx: nudged.current.dx + dx, dy: nudged.current.dy + dy };
        return;
      }
      if (!run || currentPageKey === null || busy) return;
      submit(run, currentPageKey, { kind: 'moveTextRun', page: run.pageIndex, runId: run.id, dx, dy }, run.text, dx, dy);
    },
    [run, currentPageKey, busy, content.following, submit],
  );
  useEffect(() => {
    if (busy) return;
    const { dx, dy } = nudged.current;
    if (dx === 0 && dy === 0) return;
    nudged.current = { dx: 0, dy: 0 };
    // A selection lost meanwhile (the run could not be found) drops the rest.
    if (run) move(dx, dy);
  }, [busy, run, move]);

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

  // The editor over the selected run.
  const [editor, setEditor] = useState<OpenEditor | null>(null);
  const openSeq = useRef(0);
  /**
   * Whether the open editor was already ended: a press beside it commits it
   * and, in a browser that moves the focus on that press, its blur then
   * asks again before it is closed. Only the first counts.
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
    setEditor({ run: target, pageKey: currentPageKey, style: null, family: null });
    const update = (patch: Partial<OpenEditor>) =>
      setEditor((shown) => (shown && openSeq.current === seq ? { ...shown, ...patch } : shown));
    engine.textRunStyle(handle, target.pageIndex, target.id).then(
      (style) => update({ style }),
      () => {},
    );
    const pageId = info.pageIds[target.pageIndex];
    if (pageId !== undefined) {
      void loadRunFont(engine, {
        handle,
        pageIndex: target.pageIndex,
        runId: target.id,
        pageId,
        baseGeneration,
        fontName: target.fontName,
      }).then((family) => family && update({ family }));
    }
  }, [run, currentPageKey, busy, editor, queue, engine, handle, info.pageIds, baseGeneration]);

  /** Closes the editor a turn later: never inside the blur that may have committed it. */
  const close = useCallback(() => {
    const seq = openSeq.current;
    setTimeout(() => {
      if (openSeq.current === seq) setEditor(null);
    });
  }, []);
  /** Commits `text` typed over the open editor's run, and closes it. */
  const commit = useCallback(
    (text: string, refocus: boolean) => {
      if (!editor || ended.current) return;
      ended.current = true;
      const edit = retypeOf(editor.run, text);
      if (edit) submit(editor.run, editor.pageKey, edit, edit.kind === 'removeTextRun' ? null : text);
      openSeq.current += 1;
      close();
      if (refocus) focusPages();
    },
    [editor, submit, close, focusPages],
  );
  const cancel = useCallback(() => {
    ended.current = true;
    openSeq.current += 1;
    close();
    focusPages();
  }, [close, focusPages]);

  // The text typed in the open editor, for a commit from outside it (a save,
  // leaving the mode): what is on screen goes in.
  const typed = useRef<string | null>(null);
  const shownEditor = useRef(editor);
  useLayoutEffect(() => {
    shownEditor.current = editor;
    if (!editor) typed.current = null;
  });
  const commitRef = useRef(commit);
  useLayoutEffect(() => {
    commitRef.current = commit;
  });
  /** Commits the open editor, if any (before a save). */
  const commitOpen = useCallback(() => {
    if (shownEditor.current && typed.current !== null) commitRef.current(typed.current, false);
  }, []);
  // Leaving the mode commits what was typed.
  useEffect(() => {
    if (!content.active) commitOpen();
  }, [content.active, commitOpen]);

  /** The keys the pages take for the selected run; `true` when one was used. */
  const onKey = useCallback(
    (key: string, shift: boolean): boolean => {
      if (editor) return false;
      if (!run) {
        // Only an arrow, and only while the run it moves is being found again.
        const nudge = content.following ? nudgeOf(key, shift) : null;
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
    [run, editor, content.following, open, remove, move],
  );

  return {
    run,
    look: shownLook,
    busy,
    editor,
    open,
    commit,
    cancel,
    /** The editor reports what is typed in it, so an outside commit takes it. */
    typing: (text: string) => void (typed.current = text),
    commitOpen,
    move,
    remove,
    toggle,
    onKey,
  };
}

export type TextRunEditing = ReturnType<typeof useTextRunEditing>;
