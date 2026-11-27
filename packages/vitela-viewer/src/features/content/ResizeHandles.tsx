// The resize handles of a selected image or path in the "Edit content" mode
// (B26 T-260, T-263): `useHandleDrag` follows a handle the pointer holds and
// reports the box it is let go at; `ResizeHandles` draws the handles and the
// box being dragged. Presentational: which box a drag means (an image keeps
// its proportions, a rule stays flat) is the caller's `boxOf`, and what it
// records is the container's.

import type { PageRect } from '@vitela/core';
import { useState, type PointerEvent } from 'react';
import { handleBox, type ImageHandle } from './images';
import { isDrag } from './textRuns';

const HANDLE_LABELS: Record<ImageHandle, string> = {
  nw: 'top-left corner',
  n: 'top side',
  ne: 'top-right corner',
  e: 'right side',
  se: 'bottom-right corner',
  s: 'bottom side',
  sw: 'bottom-left corner',
  w: 'left side',
};

/** A handle being dragged: where its press started, in client px. */
interface Grab {
  readonly handle: ImageHandle;
  readonly pointerId: number;
  readonly x: number;
  readonly y: number;
}

/** The box `handle` dragged by `dx, dy` points gives, Shift held or not. */
export type BoxOf = (handle: ImageHandle, dx: number, dy: number, shift: boolean) => PageRect;

/** Follows the handle the pointer holds: the box it would give now (`preview`), and the box it is let go at (`onResize`). */
export function useHandleDrag(boxOf: BoxOf, scale: number, onResize: (rect: PageRect) => void) {
  const [grab, setGrab] = useState<Grab | null>(null);
  const [preview, setPreview] = useState<PageRect | null>(null);
  const at = (held: Grab, event: PointerEvent) => boxOf(held.handle, (event.clientX - held.x) / scale, (event.clientY - held.y) / scale, event.shiftKey);
  const dragged = (held: Grab, event: PointerEvent) => preview !== null || isDrag(event.clientX - held.x, event.clientY - held.y);
  const handlers = (handle: ImageHandle) => ({
    onPointerDown: (event: PointerEvent<HTMLButtonElement>) => {
      // The handle is not the page: the press neither selects nor drags the item.
      event.stopPropagation();
      event.preventDefault();
      if (event.button !== 0 || !event.isPrimary) return;
      event.currentTarget.setPointerCapture?.(event.pointerId);
      setGrab({ handle, pointerId: event.pointerId, x: event.clientX, y: event.clientY });
    },
    onPointerMove: (event: PointerEvent<HTMLButtonElement>) => {
      event.stopPropagation();
      if (!grab || grab.pointerId !== event.pointerId) return;
      if (dragged(grab, event)) setPreview(at(grab, event));
    },
    onPointerUp: (event: PointerEvent<HTMLButtonElement>) => {
      event.stopPropagation();
      if (!grab || grab.pointerId !== event.pointerId) return;
      setGrab(null);
      setPreview(null);
      if (dragged(grab, event)) onResize(at(grab, event));
    },
    onPointerCancel: () => {
      setGrab(null);
      setPreview(null);
    },
  });
  return { preview, handlers };
}

export type HandleDrag = ReturnType<typeof useHandleDrag>;

interface ResizeHandlesProps {
  readonly rect: PageRect;
  /** CSS px per PDF point. */
  readonly scale: number;
  readonly handles: readonly ImageHandle[];
  readonly drag: HandleDrag;
  /** Another step is still running: the box is drawn, no handle is offered. */
  readonly busy: boolean;
}

export function ResizeHandles({ rect, scale, handles, drag, busy }: ResizeHandlesProps) {
  const { preview } = drag;
  return (
    <>
      {preview && (
        <div
          className="image-edit__preview"
          style={{ left: preview.left * scale, top: preview.top * scale, width: preview.width * scale, height: preview.height * scale }}
        />
      )}
      {!busy &&
        handles.map((handle) => (
          <button
            key={handle}
            type="button"
            // Pointer-only: the keyboard moves the item with the arrows.
            tabIndex={-1}
            className={`image-edit__handle image-edit__handle--${handle}`}
            aria-label={`Resize from the ${HANDLE_LABELS[handle]}`}
            style={handleBox(rect, handle, scale)}
            {...drag.handlers(handle)}
            onDoubleClick={(event) => event.stopPropagation()}
          />
        ))}
    </>
  );
}
