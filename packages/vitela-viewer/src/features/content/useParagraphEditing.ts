// Container of the "Edit content" mode's paragraph edit (B26 T-261): with a
// run selected that is part of a paragraph — stacked lines the core groups
// as one block (`readParagraphBlocks`) — retype the whole paragraph in a
// multi-line editor laid over it, as one undo step through the viewer's edit
// queue (`useAnnotationEdits.apply`), which marks the document dirty and that
// a save waits for.
//
// The rules are the core's (`applyEdit` with a `retypeTextBlock`): how the
// text wraps to the paragraph's width (each line break is the reader's), how
// lines it grows are painted, the fold of a second retype of the same
// paragraph into its step, the checkpoint. Here: what to ask for and when.
//
// - The paragraphs of the selected run's page are read once per read of
//   that page (its `contentKey`), and never while a step runs: block run ids
//   are those of the read, which a step running may be about to change.
// - The editor is opened from the run's toolbar (Edit paragraph) or with
//   Shift+Enter. Its keys are the GTK shell's: Enter is a line break,
//   Ctrl/Cmd+Enter commits, Escape cancels, leaving it commits; a save,
//   leaving the mode, a press beside it or a scroll that unmounts its page
//   commits it too (`useDraftEditor`).
// - The step checks, when its turn comes, that the page at the paragraph's
//   position is still the one it was opened on (`pageKey`), and the first
//   line is selected again once the page is read anew.
//
// The editor is drawn in the first run's size, colour and leading, in the
// family its font's name suggests (no embedded program: a paragraph's lines
// may mix fonts, and the core paints new lines in the first run's own).

import { useCallback, useEffect, useState } from 'react';
import type { ContentRun, DocumentHandle, DocumentInfo, Engine, TextBlock, TextRunStyle } from '@vitela/core';
import { pageKey } from '../annotations/useAnnotationEdits';
import { useGuardedApply, type ContentQueue } from './contentEdits';
import { paragraphOf, paragraphRetypeOf } from './textBlocks';
import type { ContentMode } from './useContentMode';
import { useDraftEditor, type Drafted } from './useDraftEditor';

/** What the paragraph editor edits: the block, where it was opened, and how to draw it. */
export interface ParagraphTarget {
  readonly block: TextBlock;
  readonly pageIndex: number;
  readonly pageKey: string;
  /** The first run's style; `null` until it is read, or when the file cannot say. */
  readonly style: TextRunStyle | null;
}

export type ParagraphEditor = Drafted<ParagraphTarget>;

const PAGE_CHANGED = 'The page changed before the paragraph could be edited. Nothing was changed.';
const NOT_RETYPABLE = 'This text cannot be retyped: its font does not say what all of its characters are.';

interface ParagraphEditingInput {
  readonly engine: Engine;
  readonly handle: DocumentHandle;
  readonly info: Pick<DocumentInfo, 'pages' | 'pageIds'>;
  readonly content: Pick<ContentMode, 'active' | 'selected' | 'following' | 'stateAt' | 'follow'>;
  readonly queue: ContentQueue;
  /** Gives the keyboard back to the pages (after Ctrl+Enter in the editor). */
  readonly focusPages: () => void;
}

/** The paragraphs of one page, as the read `key` found them. */
interface Read {
  readonly key: string;
  readonly blocks: readonly TextBlock[];
}

export function useParagraphEditing({ engine, handle, info, content, queue, focusPages }: ParagraphEditingInput) {
  const picked = content.selected;
  const run: ContentRun | null = picked?.item.kind === 'run' ? picked.item : null;
  const readKey = picked && run ? picked.key : null;
  const pageIndex = picked?.pageIndex ?? null;
  const busy = queue.pending || content.following;

  const [read, setRead] = useState<Read | null>(null);
  useEffect(() => {
    if (readKey === null || pageIndex === null || busy || read?.key === readKey) return;
    let live = true;
    engine.readParagraphBlocks(handle, pageIndex).then(
      (blocks) => live && setRead({ key: readKey, blocks }),
      () => live && setRead({ key: readKey, blocks: [] }),
    );
    return () => {
      live = false;
    };
  }, [engine, handle, readKey, pageIndex, busy, read?.key]);
  const paragraph = run && read && read.key === readKey ? paragraphOf(read.blocks, run.id) : null;

  const guarded = useGuardedApply(queue, PAGE_CHANGED);
  const { follow } = content;
  const record = useCallback(
    ({ block, pageIndex: index, pageKey: expected, draft }: ParagraphEditor) => {
      const edit = paragraphRetypeOf(index, block, draft);
      if (!edit) return;
      // The first line is where the paragraph is found again; an emptied one is gone.
      const firstLine = edit.text.trim() === '' ? null : (edit.text.split('\n')[0] ?? '');
      follow(index, block.lines[0]?.rect ?? block.rect, firstLine);
      guarded(index, expected, edit);
    },
    [follow, guarded],
  );
  const draft = useDraftEditor<ParagraphTarget>({ active: content.active, onCommit: record, focusPages });
  const { editor, open: openDraft, patch } = draft;
  const { notify } = queue;
  const { stateAt } = content;

  /** Opens the editor over the selected run's paragraph. */
  const open = useCallback(() => {
    if (!paragraph || pageIndex === null || busy || editor) return;
    const state = stateAt(pageIndex);
    const runs = state?.status === 'ready' ? state.content.runs : [];
    if (paragraph.runIds.some((id) => runs.find((candidate) => candidate.id === id)?.editable === false)) {
      notify(NOT_RETYPABLE);
      return;
    }
    const ticket = openDraft({ block: paragraph, pageIndex, pageKey: pageKey(info, pageIndex), style: null }, paragraph.text);
    const firstRun = paragraph.runIds[0];
    if (firstRun === undefined) return;
    engine.textRunStyle(handle, pageIndex, firstRun).then(
      (style) => patch(ticket, { style }),
      () => {},
    );
  }, [paragraph, pageIndex, busy, editor, stateAt, notify, openDraft, info, engine, handle, patch]);

  /** The keys the pages take for the paragraph edit; `true` when one was used. */
  const onKey = useCallback(
    (key: string, shift: boolean): boolean => {
      if (editor || !paragraph || key !== 'Enter' || !shift) return false;
      open();
      return true;
    },
    [editor, paragraph, open],
  );

  return {
    /** The selected run's paragraph, once read; `null` for a run on one line. */
    paragraph,
    busy,
    open,
    onKey,
    editor,
    commit: draft.commit,
    cancel: draft.cancel,
    typing: draft.typing,
    commitOpen: draft.commitOpen,
    attach: draft.attach,
    detach: draft.detach,
  };
}

export type ParagraphEditing = ReturnType<typeof useParagraphEditing>;
