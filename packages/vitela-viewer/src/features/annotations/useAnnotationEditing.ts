// Selecting one of the session's annotations and editing it: which one is
// selected, and what a drag, a key, the annotation toolbar and its dialogs
// ask of it, all through the viewer's queue (`useAnnotationEdits.change`).
// Container logic only.
//
// The selection is an id checked against the snapshot: once an undo, a
// delete or a removed page takes the annotation away (or a tool is armed, or
// annotating turns out to be forbidden) nothing is selected any more, and a
// redo that brings it back does not select it again.

import { useCallback, useMemo, useRef, useState } from 'react';
import { ENCODING_GAP } from '@vitela/core';
import type { Annotation } from '@vitela/core';
import { remove as removeBuild, type EditBuild } from './editBuilds';
import type { ChangeOptions, ChangeOutcome } from './useAnnotationEdits';

/** The slice of `useAnnotationEdits` this needs. */
interface AnnotationChanges {
  readonly change: (id: number, expectedKey: string, build: EditBuild, options?: ChangeOptions) => Promise<ChangeOutcome>;
}

/** What the annotations on a page call: the same object for every page. */
export interface AnnotationInput {
  readonly select: (id: number) => void;
  readonly deselect: () => void;
  /** `grabbed`: as in `ChangeOptions`. */
  readonly change: (annotation: Annotation, pageKey: string, build: EditBuild, grabbed?: Annotation) => Promise<void>;
  readonly remove: (annotation: Annotation, pageKey: string) => void;
  /** The focused annotation (or handle) went away. */
  readonly focusLost: () => void;
}

export type DialogKind = 'move' | 'resize' | 'text';

/** The annotation toolbar's open dialog. */
export interface AnnotationDialogState {
  readonly kind: DialogKind;
  readonly id: number;
  /** The change was sent and the core has not answered yet. */
  readonly busy: boolean;
  /** Why the core refused it; the dialog stays open. */
  readonly error: string | null;
}

export function useAnnotationEditing(
  edits: AnnotationChanges,
  annotations: readonly Annotation[],
  keyOf: (pageIndex: number) => string,
  /** Select is the tool and the document lets annotations be edited. */
  enabled: boolean,
  onSelect: () => void,
  focusLost: () => void,
) {
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [dialog, setDialog] = useState<AnnotationDialogState | null>(null);
  /** Counts edits typed in the dialog: a refusal describes what was sent only if none came after it. */
  const dialogEdits = useRef(0);
  const selected = (enabled && annotations.find(({ id }) => id === selectedId)) || null;
  if (selectedId !== null && !selected) setSelectedId(null);
  if (dialog && dialog.id !== selected?.id) setDialog(null);

  const { change: queue } = edits;
  const select = useCallback(
    (id: number) => {
      setSelectedId(id);
      onSelect();
    },
    [onSelect],
  );
  const deselect = useCallback(() => setSelectedId(null), []);
  const change = useCallback(
    async (annotation: Annotation, pageKey: string, build: EditBuild, grabbed?: Annotation) => {
      await queue(annotation.id, pageKey, build, { grabbed });
    },
    [queue],
  );
  /** Being removed: a second Delete before the first lands asks for nothing. */
  const removing = useRef(new Set<number>());
  const remove = useCallback(
    (annotation: Annotation, pageKey: string) => {
      if (removing.current.has(annotation.id)) return;
      removing.current.add(annotation.id);
      void queue(annotation.id, pageKey, removeBuild).finally(() => removing.current.delete(annotation.id));
    },
    [queue],
  );
  const input = useMemo<AnnotationInput | null>(
    () => (enabled ? { select, deselect, change, remove, focusLost } : null),
    [enabled, select, deselect, change, remove, focusLost],
  );

  /** A change of the selected annotation asked from its toolbar. */
  const changeSelected = (build: EditBuild) => {
    if (selected) void change(selected, keyOf(selected.pageIndex), build);
  };
  const removeSelected = () => {
    if (selected) remove(selected, keyOf(selected.pageIndex));
  };

  const openDialog = (kind: DialogKind) => {
    if (selected) setDialog({ kind, id: selected.id, busy: false, error: null });
  };
  const cancelDialog = useCallback(() => setDialog((shown) => (shown?.busy ? shown : null)), []);
  /** What was typed changed: the core's last refusal no longer describes it. */
  const dialogEdited = useCallback(() => {
    dialogEdits.current += 1;
    setDialog((shown) => (shown?.error ? { ...shown, error: null } : shown));
  }, []);
  const submitDialog = async (build: EditBuild) => {
    if (!selected || !dialog || dialog.busy) return;
    const sent: AnnotationDialogState = { ...dialog, busy: true, error: null };
    setDialog(sent);
    const typed = dialogEdits.current;
    const outcome = await queue(selected.id, keyOf(selected.pageIndex), build, { shownByCaller: [ENCODING_GAP] });
    const refused = outcome.status === 'failed' && outcome.kind === ENCODING_GAP;
    // Edited while the core answered: its refusal is of text no longer in the dialog.
    const error = refused && dialogEdits.current === typed ? outcome.message : null;
    // Only this dialog: it closed by itself if the annotation went away meanwhile.
    setDialog((shown) => {
      if (shown !== sent) return shown;
      return refused ? { ...sent, busy: false, error } : null;
    });
  };

  return {
    selected,
    deselect,
    input,
    changeSelected,
    removeSelected,
    dialog: selected && dialog?.id === selected.id ? dialog : null,
    openDialog,
    cancelDialog,
    dialogEdited,
    submitDialog,
  };
}
