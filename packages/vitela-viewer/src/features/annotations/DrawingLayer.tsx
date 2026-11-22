// The armed tool's surface over one page: takes the pointer (pointer events,
// captured for the whole gesture), shows the stroke or box being drawn, and
// hands the finished gesture to the viewer with the page it began on. Sits
// on top of the page's other layers only while a drawing tool is armed, so
// text selection is out of reach until Select is chosen again.
//
// A gesture ends on pointerup; pointercancel or a lost pointer capture drop
// it without drawing anything.
//
// Drawn in an SVG whose units are the page's drawn points, like the
// annotation layer: the pointer is divided by the zoom (`toPagePoint`) and
// the rest is `drawing.ts`.

import { memo, useRef, useState, type PointerEvent } from 'react';
import type { PagePoint, PageSize, Rgb } from '@vitela/core';
import { toPagePoint } from '../text/pageRects';
import { extendStroke, finishGesture, type ArmedTool, type Gesture } from './drawing';
import { colorCss } from './geometry';

interface DrawingLayerProps {
  readonly pageIndex: number;
  /** The page as drawn: its size in points and its turn. */
  readonly page: PageSize;
  /** What is at this position: recorded at the press, for the page-changed check. */
  readonly pageKey: string;
  /** CSS px per PDF point at the current zoom. */
  readonly scale: number;
  readonly tool: ArmedTool;
  readonly color: Rgb;
  /** A new gesture cannot start: a step is running, or annotating is not allowed. */
  readonly busy: boolean;
  readonly onGesture: (pageIndex: number, pageKey: string, tool: ArmedTool, gesture: Gesture) => void;
}

interface Stroke {
  readonly pointerId: number;
  readonly pageKey: string;
  readonly points: readonly PagePoint[];
}

export const DrawingLayer = memo(function DrawingLayer(props: DrawingLayerProps) {
  const { pageIndex, page, pageKey, scale, tool, color, busy, onGesture } = props;
  const [stroke, setStroke] = useState<Stroke | null>(null);
  // The gesture's own copy: moves in one frame must not read a stale state.
  const current = useRef<Stroke | null>(null);

  const pointOf = (event: PointerEvent<HTMLDivElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    return toPagePoint(event.clientX - box.left, event.clientY - box.top, scale);
  };
  const update = (next: Stroke | null) => {
    current.current = next;
    setStroke(next);
  };

  const end = (event: PointerEvent<HTMLDivElement>, keep: boolean) => {
    const drawn = current.current;
    if (!drawn || drawn.pointerId !== event.pointerId) return;
    event.currentTarget.releasePointerCapture?.(event.pointerId);
    const points = keep ? extendStroke(drawn.points, pointOf(event), page) : drawn.points;
    update(null);
    const gesture = keep ? finishGesture(tool, points, page) : null;
    if (gesture) onGesture(pageIndex, drawn.pageKey, tool, gesture);
  };

  // The browser took the pointer away (a system gesture, the element went
  // away): the gesture is dropped, never finished, so the next press starts.
  const lost = (event: PointerEvent<HTMLDivElement>) => {
    if (current.current?.pointerId === event.pointerId) update(null);
  };

  const preview = stroke && finishGesture(tool, stroke.points, page);
  return (
    <div
      className="drawing-layer"
      data-tool={tool}
      data-busy={busy ? 'true' : 'false'}
      onPointerDown={(event) => {
        if (busy || event.button !== 0 || !event.isPrimary || current.current) return;
        // No text selection, no focus move, no native drag while drawing.
        event.preventDefault();
        event.currentTarget.setPointerCapture?.(event.pointerId);
        update({ pointerId: event.pointerId, pageKey, points: extendStroke([], pointOf(event), page) });
      }}
      onPointerMove={(event) => {
        const drawn = current.current;
        if (!drawn || drawn.pointerId !== event.pointerId) return;
        update({ ...drawn, points: extendStroke(drawn.points, pointOf(event), page) });
      }}
      onPointerUp={(event) => end(event, true)}
      onPointerCancel={(event) => end(event, false)}
      onLostPointerCapture={lost}
    >
      {stroke && (
        <svg
          className="drawing-layer__preview"
          viewBox={`0 0 ${page.widthPt} ${page.heightPt}`}
          preserveAspectRatio="none"
          aria-hidden="true"
        >
          {tool === 'ink' ? (
            <polyline
              points={stroke.points.map(({ x, y }) => `${x},${y}`).join(' ')}
              fill="none"
              stroke={colorCss(color)}
              strokeWidth={1}
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          ) : (
            preview?.kind === 'box' && (
              <rect
                className={`drawing-layer__box drawing-layer__box--${tool}`}
                x={preview.rect.left}
                y={preview.rect.top}
                width={preview.rect.width}
                height={preview.rect.height}
                stroke={tool === 'shape' ? colorCss(color) : undefined}
              />
            )
          )}
        </svg>
      )}
    </div>
  );
});
