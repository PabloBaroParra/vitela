// One page's layer of the "Edit content" mode (B26 T-258b..T-262): over
// everything else on the page while the mode is on, it takes the pointer,
// outlines the item under it and what is selected — one item, or a group of
// several, each in its kind's style, the group inside a dashed box — and
// reports a press (with SHIFT or CTRL/⌘: a toggle), a double-click, a
// marquee dragged from where nothing is, and a drag of the selected run,
// image, path or group (its outlines follow the pointer until it is let go). What
// else the selected item has — its toolbar, its editor — comes in as
// `children`. Presentational: what is under a point, and what a drag
// records, is the container's to decide.

import type { ContentItem, PagePoint, PageRect } from '@vitela/core';
import { useRef, useState, type MouseEvent, type PointerEvent, type ReactNode } from 'react';
import { toPagePoint } from '../text/pageRects';
import { hits, itemAt, type PageContentState } from './contentModel';
import { boundsOf, marqueeRect } from './selection';
import { isDrag } from './textRuns';

/** What the content layer needs from the viewer, the same for every page. */
export interface ContentInput {
  readonly onHover: (pageIndex: number, point: PagePoint | null) => void;
  readonly onSelect: (pageIndex: number, point: PagePoint) => void;
  /** A press with SHIFT or CTRL/⌘: the item there joins the selection, or leaves it. */
  readonly onToggle: (pageIndex: number, point: PagePoint) => void;
  /** A marquee let go over `area` (drawn points); `additive` with SHIFT or CTRL/⌘ held. */
  readonly onGather: (pageIndex: number, area: PageRect, additive: boolean) => void;
  /** A double-click: select what is there and open it. */
  readonly onOpen: (pageIndex: number, point: PagePoint) => void;
  /** The selected run, image, path or group was dragged by `dx, dy` points on the page as drawn and let go. */
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
  /** The several items selected on this page, or `null`. */
  readonly group?: readonly ContentItem[] | null;
  readonly input: ContentInput;
  /** Drawn over the outlines: the selected run's toolbar and editor. */
  readonly children?: ReactNode;
}

/** A press on what is selected, which a drag may follow, or on nothing, which a marquee may. */
interface Press {
  readonly kind: 'drag' | 'marquee';
  readonly pointerId: number;
  readonly x: number;
  readonly y: number;
  readonly point: PagePoint;
  readonly additive: boolean;
}

export function ContentLayer({ pageIndex, scale, state, hovered, selected, group = null, input, children }: ContentLayerProps) {
  const pointOf = (event: MouseEvent<HTMLDivElement>): PagePoint => {
    const box = event.currentTarget.getBoundingClientRect();
    return toPagePoint(event.clientX - box.left, event.clientY - box.top, scale);
  };
  const press = useRef<Press | null>(null);
  /** How far the selection's outlines are dragged, in CSS px; `null` while they are not. */
  const [offset, setOffset] = useState<{ x: number; y: number } | null>(null);
  /** The marquee being dragged, in drawn points; `null` while there is none. */
  const [marquee, setMarquee] = useState<PageRect | null>(null);
  const groupBox = group && group.length > 0 ? boundsOf(group) : null;
  /** Whether a press at `point` is on what a drag would move: the group's box, or the selected item (a path as it is hit). */
  const onDraggable = (point: PagePoint) =>
    groupBox ? holds(groupBox, point) : selected ? (selected.kind === 'path' ? hits(selected, point) : holds(selected.rect, point)) : false;
  const moved = (event: PointerEvent<HTMLDivElement>, pressed: Press) => ({ x: event.clientX - pressed.x, y: event.clientY - pressed.y });
  return (
    <div
      className="content-layer"
      data-state={state?.status ?? 'waiting'}
      data-dragging={offset ? 'true' : undefined}
      data-marquee={marquee ? 'true' : undefined}
      onPointerMove={(event) => {
        const pressed = press.current;
        if (pressed && pressed.pointerId === event.pointerId) {
          const { x, y } = moved(event, pressed);
          if (pressed.kind === 'drag') {
            if (offset || isDrag(x, y)) setOffset({ x, y });
          } else if (marquee || isDrag(x, y)) {
            setMarquee(marqueeRect(pressed.point, pointOf(event)));
          }
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
        const additive = event.shiftKey || event.ctrlKey || event.metaKey;
        const start = (kind: Press['kind']) => {
          press.current = { kind, pointerId: event.pointerId, x: event.clientX, y: event.clientY, point, additive };
          event.currentTarget.setPointerCapture?.(event.pointerId);
        };
        if (!additive && onDraggable(point)) return start('drag');
        const onItem = state?.status === 'ready' && itemAt(state.content, point) !== null;
        if (additive && onItem) return input.onToggle(pageIndex, point);
        // From where nothing is, a drag is a marquee; a press there still lets go of the selection.
        if (!additive) input.onSelect(pageIndex, point);
        if (!onItem) start('marquee');
      }}
      onPointerUp={(event) => {
        const pressed = press.current;
        if (!pressed || pressed.pointerId !== event.pointerId) return;
        press.current = null;
        const { x, y } = moved(event, pressed);
        if (pressed.kind === 'marquee') {
          setMarquee(null);
          if (marquee || isDrag(x, y)) input.onGather(pageIndex, marqueeRect(pressed.point, pointOf(event)), pressed.additive);
          return;
        }
        setOffset(null);
        if (offset || isDrag(x, y)) input.onDrag(pageIndex, x / scale, y / scale);
        else input.onSelect(pageIndex, pressed.point);
      }}
      onPointerCancel={() => {
        press.current = null;
        setOffset(null);
        setMarquee(null);
      }}
      onDoubleClick={(event) => input.onOpen(pageIndex, pointOf(event))}
    >
      {hovered && !offset && !marquee && !sameBox(hovered, selected) && !group?.some((item) => sameBox(hovered, item)) && (
        <Outline item={hovered} scale={scale} state="hover" />
      )}
      {selected && <Outline item={selected} scale={scale} state="selected" offset={offset} />}
      {group?.map((item) => <Outline key={`${item.kind}:${item.id}`} item={item} scale={scale} state="selected" offset={offset} />)}
      {groupBox && <Box rect={groupBox} scale={scale} className="content-group" offset={offset} />}
      {marquee && <Box rect={marquee} scale={scale} className="content-marquee" />}
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

function Box({ rect, scale, className, offset = null }: { rect: PageRect; scale: number; className: string; offset?: { x: number; y: number } | null }) {
  return (
    <div
      className={className}
      style={{
        left: rect.left * scale,
        top: rect.top * scale,
        width: rect.width * scale,
        height: rect.height * scale,
        transform: offset ? `translate(${offset.x}px, ${offset.y}px)` : undefined,
      }}
    />
  );
}

function holds(rect: PageRect, point: PagePoint): boolean {
  const { left, top, width, height } = rect;
  return point.x >= left && point.x <= left + width && point.y >= top && point.y <= top + height;
}

function sameBox(a: ContentItem, b: ContentItem | null): boolean {
  return !!b && a.kind === b.kind && a.id === b.id;
}
