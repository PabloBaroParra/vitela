// Dragging cards to reorder them, with pointer events rather than HTML5
// drag-and-drop: one code path for mouse, pen and touch, and the drop slot
// comes from the grid's own layout (`dropSlotAt`), not from whichever DOM
// node the pointer happens to be over. Container logic only.
//
// A press becomes a drag after it travels `DRAG_THRESHOLD_PX`; until then it
// is a click (selection). The click that a browser fires after a drag is
// swallowed. Near the top or bottom edge the scroller scrolls on each move.

import { useCallback, useRef, useState, type PointerEvent, type RefObject } from 'react';
import { dropSlotAt, type DropSlot, type GridLayout } from './gridLayout';

const DRAG_THRESHOLD_PX = 6;
/** How close to the scroller's edge a drag scrolls it, and by how much per move. */
const EDGE_PX = 40;
const EDGE_SCROLL_PX = 24;

interface Press {
  readonly pointerId: number;
  readonly x: number;
  readonly y: number;
  readonly index: number;
  dragging: boolean;
}

interface ReorderDragOptions {
  readonly scroller: RefObject<HTMLElement | null>;
  readonly content: RefObject<HTMLElement | null>;
  readonly layout: GridLayout;
  readonly enabled: boolean;
  /** The press on card `index` became a drag (select it, if it was not). */
  readonly onStart: (index: number) => void;
  /** Dropped in front of page `slot` (`pageCount` = after the last). */
  readonly onDrop: (slot: number) => void;
}

export function useReorderDrag({ scroller, content, layout, enabled, onStart, onDrop }: ReorderDragOptions) {
  const press = useRef<Press | null>(null);
  const [slot, setSlot] = useState<DropSlot | null>(null);
  const swallowClick = useRef(false);

  const slotAt = useCallback(
    (event: PointerEvent) => {
      const box = content.current?.getBoundingClientRect();
      return box ? dropSlotAt(layout, { x: event.clientX - box.left, y: event.clientY - box.top }) : null;
    },
    [content, layout],
  );

  const onCardPointerDown = useCallback(
    (index: number, event: PointerEvent) => {
      // A drag captures the pointer, so the click after it may land on the
      // grid rather than a card and never be taken: a new press ends that wait.
      swallowClick.current = false;
      if (!enabled || event.button !== 0 || event.shiftKey || event.ctrlKey || event.metaKey) return;
      press.current = { pointerId: event.pointerId, x: event.clientX, y: event.clientY, index, dragging: false };
    },
    [enabled],
  );

  const onPointerMove = useCallback(
    (event: PointerEvent<HTMLElement>) => {
      const current = press.current;
      if (!current || current.pointerId !== event.pointerId) return;
      if (!current.dragging) {
        if (Math.hypot(event.clientX - current.x, event.clientY - current.y) < DRAG_THRESHOLD_PX) return;
        current.dragging = true;
        event.currentTarget.setPointerCapture?.(event.pointerId);
        onStart(current.index);
      }
      const element = scroller.current;
      if (element) {
        const box = element.getBoundingClientRect();
        if (event.clientY < box.top + EDGE_PX) element.scrollTop -= EDGE_SCROLL_PX;
        else if (event.clientY > box.bottom - EDGE_PX) element.scrollTop += EDGE_SCROLL_PX;
      }
      setSlot(slotAt(event));
    },
    [onStart, scroller, slotAt],
  );

  const end = useCallback(
    (event: PointerEvent<HTMLElement>, drop: boolean) => {
      const current = press.current;
      if (!current || current.pointerId !== event.pointerId) return;
      press.current = null;
      if (!current.dragging) return;
      swallowClick.current = true;
      setSlot(null);
      const target = drop ? slotAt(event) : null;
      if (target) onDrop(target.index);
    },
    [onDrop, slotAt],
  );

  const onPointerUp = useCallback((event: PointerEvent<HTMLElement>) => end(event, true), [end]);
  const onPointerCancel = useCallback((event: PointerEvent<HTMLElement>) => end(event, false), [end]);

  /** True once for the click that ends a drag. */
  const takeSwallowedClick = useCallback(() => {
    const swallowed = swallowClick.current;
    swallowClick.current = false;
    return swallowed;
  }, []);

  return { slot, onCardPointerDown, onPointerMove, onPointerUp, onPointerCancel, takeSwallowedClick };
}
