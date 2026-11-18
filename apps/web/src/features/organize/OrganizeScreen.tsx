// Container: the Organize screen — every page (Pages view) or every block of
// pages from one PDF (Documents view) as a thumbnail, to reorder (drag, or
// Ctrl+Shift+arrow), rotate and delete, plus Add PDFs (picker or drop), with
// undo/redo. The selection is one contiguous run of cards, which is always one
// contiguous run of pages, so each operation is one core command and one undo
// step in either view. The rules (permission, positions, what an import may
// carry) are the core's; this screen keeps the selection in step with what an
// edit did.

import { useCallback, useMemo, useRef, useState, type DragEvent, type KeyboardEvent } from 'react';
import type { DocumentHandle, EditedDocument, Engine, ImportedDocument, PageEdit } from '@vitela/core';
import { StatusLine } from '../../ui/StatusLine';
import type { OpenDocument } from '../document/useDocument';
import { carriesFiles } from '../document/droppedFile';
import { useMarkDirty } from '../save/MarkDirtyContext';
import { BlockCard, blockCardId } from './BlockCard';
import type { CardPage } from './CardThumbnail';
import type { GridKeyAction } from './gridKeys';
import { ImportStatus } from './ImportStatus';
import { moveUnitsBy, moveUnitsToSlot, runAfterMove, type MoveEdit } from './moves';
import { OrganizeToolbar } from './OrganizeToolbar';
import { cardId, PageCard } from './PageCard';
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
import { ThumbnailGrid, type GridItem } from './ThumbnailGrid';
import { blockLabel, unitsOf, type OrganizeView, type Units } from './units';
import { useImport, type PreparedSource } from './useImport';
import { useOrganize } from './useOrganize';

interface OrganizeScreenProps {
  readonly engine: Engine;
  readonly document: OpenDocument;
  /**
   * Hands the edited document to the workspace; `false` when that document
   * is no longer the one on screen (then it is not marked dirty either).
   */
  readonly onEdited: (handle: DocumentHandle, edited: EditedDocument) => boolean;
  /** Records which file each imported PDF came from, by its block source id. */
  readonly onSourcesNamed: (handle: DocumentHandle, names: readonly (readonly [number, string])[]) => void;
}

const NO_NAMES: ReadonlyMap<number, string> = new Map();

export function OrganizeScreen({ engine, document, onEdited, onSourcesNamed }: OrganizeScreenProps) {
  const markDirty = useMarkDirty();
  const { handle, info } = document;
  const report = useCallback(
    (edited: EditedDocument) => {
      if (onEdited(handle, edited)) markDirty();
    },
    [onEdited, handle, markDirty],
  );
  const organize = useOrganize(engine, document, report);
  // One cache per screen: thumbnails are kept by page id across every edit and both views.
  const cache = useMemo(() => new ThumbnailCache(), []);
  const [view, setView] = useState<OrganizeView>('pages');
  /** The view as of the last render, for an import that lands later. */
  const viewNow = useRef(view);
  viewNow.current = view;
  const units = unitsOf(view, info);
  const [chosen, setChosen] = useState<Selection | null>(() => (info.pageCount > 0 ? selectOnly(0) : null));
  const selection = clampSelection(chosen, units.count);
  const run = selection && runOf(selection);
  const pages = run && units.pageRun(run);
  const canEdit = organize.allowed && !organize.pending && pages !== null;

  /** Applies `edit` and, once the core took it, selects what `next` says in the edited document. */
  const edit = useCallback(
    async (pageEdit: PageEdit, next: (edited: EditedDocument, units: Units) => Selection | null) => {
      const edited = await organize.apply(pageEdit);
      if (edited) setChosen(next(edited, unitsOf(view, edited.info)));
    },
    [organize, view],
  );

  const importAll = useCallback(
    async (sources: readonly PreparedSource[]) => {
      const imported: ImportedDocument | null = await organize.importPrepared(sources.map(({ id }) => id));
      if (!imported) return false;
      onSourcesNamed(
        handle,
        imported.sourceIds.map((id, index) => [id, sources[index]!.name] as const),
      );
      // The added pages are the last ones: select them.
      const count = sources.reduce((sum, source) => sum + source.pageCount, 0);
      const added = { from: imported.info.pageCount - count, count };
      if (count > 0 && added.from >= 0) setChosen(selectRun(unitsOf(viewNow.current, imported.info).covering(added)));
      return true;
    },
    [organize, onSourcesNamed, handle],
  );
  const adding = useImport(engine, importAll);
  const addRefusal = organize.allowed ? info.importRefusal : (info.importRefusal ?? 'This PDF does not allow adding pages.');
  const addFiles = (files: readonly File[]) => {
    if (addRefusal === null && !adding.busy) void adding.add(files);
  };

  const rotate = (degrees: number) => {
    if (canEdit && pages) void edit({ kind: 'rotate', ...pages, degrees }, () => selection);
  };
  const remove = () => {
    if (!canEdit || !pages || !run) return;
    // The card after the removed run takes its place, or the new last card.
    void edit({ kind: 'remove', ...pages }, (_edited, after) =>
      after.count > 0 ? selectOnly(Math.min(run.from, after.count - 1)) : null,
    );
  };
  const move = (pageEdit: MoveEdit | null) => {
    if (canEdit && pageEdit) void edit(pageEdit, (_edited, after) => selectRun(after.covering(runAfterMove(pageEdit))));
  };
  const moveSelectionBy = (delta: number) => run && move(moveUnitsBy(units, run, delta));
  const undo = () => void organize.undo();
  const redo = () => void organize.redo();

  const switchView = (next: OrganizeView) => {
    if (next === view) return;
    // The same pages stay selected: as the cards of the other view that hold them.
    setChosen(pages && pages.count > 0 ? selectRun(unitsOf(next, info).covering(pages)) : chosen);
    setView(next);
  };

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
  const onDrop = (slot: number) => run && move(moveUnitsToSlot(units, run, slot));
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

  // Files dropped on this screen are added to the document rather than
  // opened in its place: claiming the drop (`preventDefault`) tells the
  // workspace's window-wide listener, which sees it next, to open nothing.
  const onFileDrop = (event: DragEvent) => {
    if (!carriesFiles([...event.dataTransfer.types])) return;
    event.preventDefault();
    addFiles([...event.dataTransfer.files]);
  };

  const thumbnail = (pageIndex: number): CardPage => ({
    engine,
    document: handle,
    cache,
    pageIndex,
    pageId: info.pageIds[pageIndex]!,
    page: info.pages[pageIndex]!,
    devicePixelRatio: typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1,
    paused: organize.pending,
  });
  const grid =
    view === 'pages'
      ? {
          label: `Pages of ${document.name}`,
          itemKey: (index: number) => info.pageIds[index]!,
          itemId: (index: number) => cardId(info.pageIds[index]!),
          renderItem: (item: GridItem) => <PageCard {...item} thumbnail={thumbnail(item.index)} />,
        }
      : {
          label: `Documents in ${document.name}`,
          itemKey: (index: number) => info.pageIds[info.blocks[index]!.start]!,
          itemId: (index: number) => blockCardId(info.pageIds[info.blocks[index]!.start]!),
          renderItem: (item: GridItem) => {
            const block = info.blocks[item.index]!;
            return (
              <BlockCard
                {...item}
                thumbnail={thumbnail(block.start)}
                label={blockLabel(block, document.name, document.sourceNames ?? NO_NAMES)}
                pageCount={block.count}
              />
            );
          },
        };

  return (
    <section
      className="organize"
      aria-label={`Organize ${document.name}`}
      onKeyDown={onKeyDown}
      onDrop={onFileDrop}
    >
      <OrganizeToolbar
        view={view}
        onViewChange={switchView}
        addRefusal={addRefusal}
        adding={adding.busy}
        onAddFiles={addFiles}
        summary={summary(view, run, pages, units.count, info.pageCount)}
        canEdit={canEdit}
        canMoveEarlier={!!run && run.from > 0}
        canMoveLater={!!run && run.from + run.count < units.count}
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
      <ImportStatus
        activity={adding.activity}
        onSubmitPassword={adding.submitPassword}
        onCancelPassword={adding.cancelPassword}
      />
      {organize.failure && <StatusLine tone="error">{`Could not change the pages: ${organize.failure}`}</StatusLine>}
      <ThumbnailGrid
        {...grid}
        // A new grid per view: its scroll and drag state belong to the cards it showed.
        key={view}
        count={units.count}
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

function summary(
  view: OrganizeView,
  run: PageRun | null,
  pages: PageRun | null,
  unitCount: number,
  pageCount: number,
): string {
  if (view === 'pages') {
    if (!run) return `No page selected (${pageCount} pages)`;
    const of = `of ${pageCount}`;
    return run.count === 1
      ? `Page ${run.from + 1} ${of} selected`
      : `Pages ${run.from + 1}–${run.from + run.count} ${of} selected`;
  }
  const documents = `${unitCount} ${unitCount === 1 ? 'document' : 'documents'}`;
  if (!run || !pages) return `No document selected (${documents})`;
  const which = run.count === 1 ? `Document ${run.from + 1}` : `Documents ${run.from + 1}–${run.from + run.count}`;
  return `${which} of ${unitCount} selected (${pages.count} ${pages.count === 1 ? 'page' : 'pages'})`;
}
