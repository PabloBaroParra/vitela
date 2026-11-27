// One page's layer of the "Edit content" mode (B26 T-258b, T-259): over
// everything else on the page while the mode is on, it takes the pointer,
// outlines the item under it and the one selected — a text run, an image or
// a shape, each in its own style — and reports a press, a double-click, and
// a drag of the selected run (its outline follows the pointer until it is
// let go). What else the selected item has — its toolbar, its editor —
// comes in as `children`. Presentational: what is under a point, and what a
// drag records, is the container's to decide.

import type { ContentItem, PagePoint } from '@vitela/core';
import { useRef, useState, type MouseEvent, type ReactNode } from 'react';
import { toPagePoint } from '../text/pageRects';
import type { PageContentState } from './contentModel';
import { isDrag } from './textRuns';

/** What the content layer needs from the viewer, the same for every page. */
export interface ContentInput {
  readonly onHover: (pageIndex: number, point: PagePoint | null) => void;
  readonly onSelect: (pageIndex: number, point: PagePoint) => void;
  /** A double-click: select what is there and open it. */
  readonly onOpen: (pageIndex: number, point: PagePoint) => void;
  /** The selected run was dragged by `dx, dy` points on the page as drawn and let go. */
  readonly onDrag: (pageIndex: number, dx: number, dy: number) => void;
}

interface ContentLayerProps {
  readonly pageIndex: number;
  /** CSS px per PDF point. */
  readonly scale: number;
  /** This page's parse; absent before it is asked for. */
  readonly state: PageContentState | undefined;
  readonly hovered: ContentItem | null;
  readonly selected: ContentItem | null;
  readonly input: ContentInput;
  /** Drawn over the outlines: the selected run's toolbar and editor. */
  readonly children?: ReactNode;
}

/** A press on the selected run, which a drag may follow. */
interface Press {
  readonly pointerId: number;
  readonly x: number;
  readonly y: number;
  readonly point: PagePoint;
}

export function ContentLayer({ pageIndex, scale, state, hovered, selected, input, children }: ContentLayerProps) {
  const pointOf = (event: MouseEvent<HTMLDivElement>): PagePoint => {
    const box = event.currentTarget.getBoundingClientRect();
    return toPagePoint(event.clientX - box.left, event.clientY - box.top, scale);
  };
  const press = useRef<Press | null>(null);
  /** How far the selected run's outline is dragged, in CSS px; `null` while it is not. */
  const [offset, setOffset] = useState<{ x: number; y: number } | null>(null);
  const draggable = selected?.kind === 'run' ? selected : null;
  return (
    <div
      className="content-layer"
      data-state={state?.status ?? 'waiting'}
      data-dragging={offset ? 'true' : undefined}
      onPointerMove={(event) => {
        const pressed = press.current;
        if (pressed && pressed.pointerId === event.pointerId) {
          const x = event.clientX - pressed.x;
          const y = event.clientY - pressed.y;
          if (offset || isDrag(x, y)) setOffset({ x, y });
          return;
        }
        input.onHover(pageIndex, pointOf(event));
      }}
      onPointerLeave={() => input.onHover(pageIndex, null)}
      onPointerDown={(event) => {
        if (event.button !== 0 || !event.isPrimary) return;
        // The press is the mode's: no text selection starts under it.
        event.preventDefault();
        const point = pointOf(event);
        if (draggable && holds(draggable, point)) {
          press.current = { pointerId: event.pointerId, x: event.clientX, y: event.clientY, point };
          event.currentTarget.setPointerCapture?.(event.pointerId);
          return;
        }
        input.onSelect(pageIndex, point);
      }}
      onPointerUp={(event) => {
        const pressed = press.current;
        if (!pressed || pressed.pointerId !== event.pointerId) return;
        press.current = null;
        const x = event.clientX - pressed.x;
        const y = event.clientY - pressed.y;
        setOffset(null);
        if (offset || isDrag(x, y)) input.onDrag(pageIndex, x / scale, y / scale);
        else input.onSelect(pageIndex, pressed.point);
      }}
      onPointerCancel={() => {
        press.current = null;
        setOffset(null);
      }}
      onDoubleClick={(event) => input.onOpen(pageIndex, pointOf(event))}
    >
      {hovered && !offset && !sameBox(hovered, selected) && <Outline item={hovered} scale={scale} state="hover" />}
      {selected && <Outline item={selected} scale={scale} state="selected" offset={offset} />}
      {!offset && children}
    </div>
  );
}

function Outline({
  item,
  scale,
  state,
  offset = null,
}: {
  item: ContentItem;
  scale: number;
  state: 'hover' | 'selected';
  offset?: { x: number; y: number } | null;
}) {
  const { left, top, width, height } = item.rect;
  return (
    <div
      className={`content-outline content-outline--${item.kind} content-outline--${state}`}
      data-kind={item.kind}
      data-state={state}
      style={{
        left: left * scale,
        top: top * scale,
        width: width * scale,
        height: height * scale,
        transform: offset ? `translate(${offset.x}px, ${offset.y}px)` : undefined,
      }}
    />
  );
}

function holds(item: ContentItem, point: PagePoint): boolean {
  const { left, top, width, height } = item.rect;
  return point.x >= left && point.x <= left + width && point.y >= top && point.y <= top + height;
}

function sameBox(a: ContentItem, b: ContentItem | null): boolean {
  return !!b && a.kind === b.kind && a.id === b.id;
}
