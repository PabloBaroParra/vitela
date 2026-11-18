// The Organize grid: one card per unit — a page (Pages view) or a block
// (Documents view) — virtualized by row (only the rows within a viewport of
// the visible area are mounted, so only they render), with pointer drag to
// reorder and a listbox keyboard. The grid is the focus target; the active
// card is named by `aria-activedescendant`, so a card can be scrolled out and
// unmounted without the focus being lost. What a card shows is the caller's
// (`renderItem`).

import { Fragment, useLayoutEffect, useMemo, useRef, type MouseEvent, type PointerEvent, type ReactNode } from 'react';
import type { Rect } from '../viewer/layout';
import { revealScroll } from '../viewer/reveal';
import { useScrollMetrics } from '../viewer/useScrollMetrics';
import { cardRect, dropMarkerRect, layoutGrid, visibleCards } from './gridLayout';
import { gridKeyAction, type GridKeyAction } from './gridKeys';
import { contains, type Selection } from './selection';
import { useReorderDrag } from './useReorderDrag';

/** Room kept around the active card scrolled into view, in CSS px. */
const REVEAL_MARGIN = 16;

/** What the grid tells a card it renders. */
export interface GridItem {
  readonly index: number;
  readonly rect: Rect;
  readonly selected: boolean;
  readonly active: boolean;
  readonly onPointerDown: (index: number, event: PointerEvent) => void;
  readonly onClick: (index: number, event: MouseEvent) => void;
}

interface ThumbnailGridProps {
  /** The listbox's accessible name. */
  readonly label: string;
  /** How many cards. */
  readonly count: number;
  /** A card's React key: stable for the same page or block across edits. */
  readonly itemKey: (index: number) => string | number;
  /** A card's DOM id, for `aria-activedescendant`. */
  readonly itemId: (index: number) => string;
  readonly renderItem: (item: GridItem) => ReactNode;
  readonly selection: Selection | null;
  /** An edit is in flight: no drags. */
  readonly paused: boolean;
  /** Whether cards may be dragged (the document permits assembly). */
  readonly canReorder: boolean;
  readonly onSelect: (index: number, extend: boolean) => void;
  /** A drag started on card `index`. */
  readonly onDragStart: (index: number) => void;
  /** The selection was dropped in front of card `slot`. */
  readonly onDrop: (slot: number) => void;
  /** A move or delete key (focus keys are handled here, as `onSelect`). */
  readonly onKeyAction: (action: Exclude<GridKeyAction, { kind: 'focus' }>) => void;
}

export function ThumbnailGrid(props: ThumbnailGridProps) {
  const { label, count, itemKey, itemId, renderItem, selection, paused, canReorder } = props;
  const { onSelect, onDragStart, onDrop, onKeyAction } = props;
  const scroller = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const metrics = useScrollMetrics(scroller);
  const layout = useMemo(() => layoutGrid(count, metrics.width), [count, metrics.width]);
  const range = visibleCards(layout, metrics.top, metrics.height, metrics.height);
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
      cards.push(
        <Fragment key={itemKey(index)}>
          {renderItem({
            index,
            rect: cardRect(layout, index),
            selected: !!selection && contains(selection, index),
            active: focus === index,
            onPointerDown: drag.onCardPointerDown,
            onClick,
          })}
        </Fragment>,
      );
    }
  }
  const active = focus !== null && focus < count ? itemId(focus) : undefined;

  return (
    <div
      ref={scroller}
      className="organize__grid"
      role="listbox"
      aria-multiselectable="true"
      aria-label={label}
      aria-activedescendant={active}
      tabIndex={0}
      data-dragging={drag.slot ? 'true' : 'false'}
      onKeyDown={(event) => {
        const action = gridKeyAction(event, { focus, columns: layout.columns, pageCount: count });
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
