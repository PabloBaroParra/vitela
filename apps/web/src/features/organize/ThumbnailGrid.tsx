// The Organize grid: one card per page, virtualized by row (only the rows
// within a viewport of the visible area are mounted, so only they render),
// with pointer drag to reorder and a listbox keyboard. The grid is the focus
// target; the active card is named by `aria-activedescendant`, so a card can
// be scrolled out and unmounted without the focus being lost.

import { useLayoutEffect, useMemo, useRef, type MouseEvent } from 'react';
import type { DocumentHandle, DocumentInfo, Engine } from '@vitela/core';
import { revealScroll } from '../viewer/reveal';
import { useScrollMetrics } from '../viewer/useScrollMetrics';
import { cardRect, dropMarkerRect, layoutGrid, visibleCards } from './gridLayout';
import { gridKeyAction, type GridKeyAction } from './gridKeys';
import { cardId, PageCard } from './PageCard';
import { contains, type Selection } from './selection';
import type { ThumbnailCache } from './thumbnailCache';
import { useReorderDrag } from './useReorderDrag';

/** Room kept around the active card scrolled into view, in CSS px. */
const REVEAL_MARGIN = 16;

interface ThumbnailGridProps {
  readonly engine: Engine;
  readonly document: DocumentHandle;
  readonly documentName: string;
  readonly info: DocumentInfo;
  readonly cache: ThumbnailCache;
  readonly selection: Selection | null;
  /** An edit is in flight: no new renders, no drags. */
  readonly paused: boolean;
  /** Whether pages may be dragged (the document permits assembly). */
  readonly canReorder: boolean;
  readonly onSelect: (index: number, extend: boolean) => void;
  /** A drag started on card `index`. */
  readonly onDragStart: (index: number) => void;
  /** The selection was dropped in front of page `slot`. */
  readonly onDrop: (slot: number) => void;
  /** A move or delete key (focus keys are handled here, as `onSelect`). */
  readonly onKeyAction: (action: Exclude<GridKeyAction, { kind: 'focus' }>) => void;
}

export function ThumbnailGrid(props: ThumbnailGridProps) {
  const { engine, document, documentName, info, cache, selection, paused, canReorder } = props;
  const { onSelect, onDragStart, onDrop, onKeyAction } = props;
  const scroller = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const metrics = useScrollMetrics(scroller);
  const layout = useMemo(() => layoutGrid(info.pageCount, metrics.width), [info.pageCount, metrics.width]);
  const range = visibleCards(layout, metrics.top, metrics.height, metrics.height);
  const pixelRatio = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
  const drag = useReorderDrag({
    scroller,
    content,
    layout,
    enabled: canReorder && !paused,
    onStart: onDragStart,
    onDrop,
  });

  // The active card stays in view, wherever the keyboard or an edit put it.
  const focus = selection?.focus ?? null;
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element || focus === null || focus >= layout.pageCount || element.clientHeight === 0) return;
    const viewport = {
      left: element.scrollLeft,
      top: element.scrollTop,
      width: element.clientWidth,
      height: element.clientHeight,
    };
    const scroll = revealScroll(viewport, cardRect(layout, focus), REVEAL_MARGIN);
    if (scroll) element.scrollTop = scroll.y;
  }, [focus, layout]);

  const onClick = (index: number, event: MouseEvent) => {
    if (drag.takeSwallowedClick()) return;
    onSelect(index, event.shiftKey);
  };

  const cards = [];
  if (range && metrics.height > 0) {
    for (let index = range.first; index <= range.last; index++) {
      const pageId = info.pageIds[index]!;
      cards.push(
        <PageCard
          key={pageId}
          engine={engine}
          document={document}
          cache={cache}
          index={index}
          pageId={pageId}
          page={info.pages[index]!}
          rect={cardRect(layout, index)}
          devicePixelRatio={pixelRatio}
          selected={!!selection && contains(selection, index)}
          active={focus === index}
          paused={paused}
          onPointerDown={drag.onCardPointerDown}
          onClick={onClick}
        />,
      );
    }
  }
  const active = focus !== null && info.pageIds[focus] !== undefined ? cardId(info.pageIds[focus]!) : undefined;

  return (
    <div
      ref={scroller}
      className="organize__grid"
      role="listbox"
      aria-multiselectable="true"
      aria-label={`Pages of ${documentName}`}
      aria-activedescendant={active}
      tabIndex={0}
      data-dragging={drag.slot ? 'true' : 'false'}
      onKeyDown={(event) => {
        const action = gridKeyAction(event, { focus, columns: layout.columns, pageCount: info.pageCount });
        if (!action) return;
        event.preventDefault();
        if (action.kind === 'focus') onSelect(action.index, action.extend);
        else onKeyAction(action);
      }}
      onPointerMove={drag.onPointerMove}
      onPointerUp={drag.onPointerUp}
      onPointerCancel={drag.onPointerCancel}
    >
      <div ref={content} className="organize__content" style={{ width: layout.width, height: layout.height }}>
        {cards}
        {drag.slot && <div className="organize__drop-marker" style={dropMarkerRect(drag.slot)} aria-hidden="true" />}
      </div>
    </div>
  );
}
