// Container: the Organize screen — every page as a thumbnail, to reorder
// (drag, or Ctrl+Shift+arrow), rotate and delete, with undo/redo. The
// selection is one contiguous run, so each operation is one core command and
// one undo step. The rules (permission, positions) are the core's; this
// screen keeps the selection in step with what an edit did.

import { useCallback, useMemo, useState, type KeyboardEvent } from 'react';
import type { DocumentHandle, EditedDocument, Engine, PageEdit } from '@vitela/core';
import { StatusLine } from '../../ui/StatusLine';
import type { OpenDocument } from '../document/useDocument';
import { useMarkDirty } from '../save/MarkDirtyContext';
import type { GridKeyAction } from './gridKeys';
import { moveBy, moveToSlot, runAfterMove, type MoveEdit } from './moves';
import { OrganizeToolbar } from './OrganizeToolbar';
import {
  clampSelection,
  contains,
  extendTo,
  runOf,
  selectOnly,
  selectRun,
  type PageRun,
  type Selection,
} from './selection';
import { ThumbnailCache } from './thumbnailCache';
import { ThumbnailGrid } from './ThumbnailGrid';
import { useOrganize } from './useOrganize';

interface OrganizeScreenProps {
  readonly engine: Engine;
  readonly document: OpenDocument;
  /**
   * Hands the edited document to the workspace; `false` when that document
   * is no longer the one on screen (then it is not marked dirty either).
   */
  readonly onEdited: (handle: DocumentHandle, edited: EditedDocument) => boolean;
}

export function OrganizeScreen({ engine, document, onEdited }: OrganizeScreenProps) {
  const markDirty = useMarkDirty();
  const { handle, info } = document;
  const report = useCallback(
    (edited: EditedDocument) => {
      if (onEdited(handle, edited)) markDirty();
    },
    [onEdited, handle, markDirty],
  );
  const organize = useOrganize(engine, document, report);
  // One cache per screen: thumbnails are kept by page id across every edit.
  const cache = useMemo(() => new ThumbnailCache(), []);
  const [chosen, setChosen] = useState<Selection | null>(() => (info.pageCount > 0 ? selectOnly(0) : null));
  const selection = clampSelection(chosen, info.pageCount);
  const run = selection && runOf(selection);
  const canEdit = organize.allowed && !organize.pending && run !== null;

  /** Applies `edit` and, once the core took it, selects what `next` says. */
  const edit = useCallback(
    async (pageEdit: PageEdit, next: (edited: EditedDocument) => Selection | null) => {
      const edited = await organize.apply(pageEdit);
      if (edited) setChosen(next(edited));
    },
    [organize],
  );

  const rotate = (degrees: number) => {
    if (canEdit && run) void edit({ kind: 'rotate', ...run, degrees }, () => selection);
  };
  const remove = () => {
    if (!canEdit || !run) return;
    // The page after the removed run takes its place, or the new last page.
    void edit({ kind: 'remove', ...run }, ({ info }) =>
      info.pageCount > 0 ? selectOnly(Math.min(run.from, info.pageCount - 1)) : null,
    );
  };
  const move = (pageEdit: MoveEdit | null) => {
    if (canEdit && pageEdit) void edit(pageEdit, () => selectRun(runAfterMove(pageEdit)));
  };
  const moveSelectionBy = (delta: number) => run && move(moveBy(run, delta, info.pageCount));
  const undo = () => void organize.undo();
  const redo = () => void organize.redo();

  const onSelect = useCallback(
    (index: number, extend: boolean) => setChosen((prior) => (extend && prior ? extendTo(prior, index) : selectOnly(index))),
    [],
  );
  // Dragging an unselected card drags it alone.
  const onDragStart = useCallback(
    (index: number) => setChosen((prior) => (prior && contains(prior, index) ? prior : selectOnly(index))),
    [],
  );
  // A fresh closure every render: at drop time it sees the selection the
  // drag's start may just have changed.
  const onDrop = (slot: number) => run && move(moveToSlot(run, slot));
  const onKeyAction = (action: Exclude<GridKeyAction, { kind: 'focus' }>) => {
    if (action.kind === 'delete') remove();
    else moveSelectionBy(action.delta);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
    const key = event.key.toLowerCase();
    const isRedo = (key === 'z' && event.shiftKey) || (key === 'y' && !event.shiftKey);
    const isUndo = key === 'z' && !event.shiftKey;
    if (!isUndo && !isRedo) return;
    event.preventDefault();
    if (isUndo) undo();
    else redo();
  };

  return (
    <section className="organize" aria-label={`Organize ${document.name}`} onKeyDown={onKeyDown}>
      <OrganizeToolbar
        summary={summary(run, info.pageCount)}
        canEdit={canEdit}
        canMoveEarlier={!!run && run.from > 0}
        canMoveLater={!!run && run.from + run.count < info.pageCount}
        canUndo={organize.canUndo && !organize.pending}
        canRedo={organize.canRedo && !organize.pending}
        onRotateLeft={() => rotate(-90)}
        onRotateRight={() => rotate(90)}
        onDelete={remove}
        onMoveEarlier={() => moveSelectionBy(-1)}
        onMoveLater={() => moveSelectionBy(1)}
        onUndo={undo}
        onRedo={redo}
      />
      {!organize.allowed && (
        <p className="organize__blocked">
          This PDF does not allow its pages to be moved, rotated or deleted. Open it with its owner password to
          organize it.
        </p>
      )}
      {organize.failure && <StatusLine tone="error">{`Could not change the pages: ${organize.failure}`}</StatusLine>}
      <ThumbnailGrid
        engine={engine}
        document={handle}
        documentName={document.name}
        info={info}
        cache={cache}
        selection={selection}
        paused={organize.pending}
        canReorder={organize.allowed}
        onSelect={onSelect}
        onDragStart={onDragStart}
        onDrop={onDrop}
        onKeyAction={onKeyAction}
      />
    </section>
  );
}

function summary(run: PageRun | null, pageCount: number): string {
  if (!run) return `No page selected (${pageCount} pages)`;
  const of = `of ${pageCount}`;
  return run.count === 1
    ? `Page ${run.from + 1} ${of} selected`
    : `Pages ${run.from + 1}–${run.from + run.count} ${of} selected`;
}
