// The text layer over one page: search highlights, the selection, and the
// pointer input that selects text. Lives inside the page's slot, so it is
// mounted and unmounted with the page. Rects come from the engine already on
// the page as drawn; this only scales them by the zoom.

import { memo, useRef, type PointerEvent, type MouseEvent } from 'react';
import type { PageRects } from '../../core/types';
import { toCssRects, toPagePoint } from './pageRects';
import type { TextInput } from './useTextSelection';

interface TextLayerProps {
  readonly pageIndex: number;
  /** CSS px per PDF point at the current zoom. */
  readonly scale: number;
  /** This page's search matches, one `PageRects` per match. */
  readonly hits?: readonly PageRects[];
  /** The current match, when it is on this page (one of `hits`). */
  readonly currentHit?: PageRects;
  readonly selection?: PageRects;
  /** `null` while the document withholds its text: nothing is selectable. */
  readonly input: TextInput | null;
}

export const TextLayer = memo(function TextLayer(props: TextLayerProps) {
  const { pageIndex, scale, hits, currentHit, selection, input } = props;
  const dragging = useRef(false);

  const pointOf = (event: PointerEvent<HTMLDivElement> | MouseEvent<HTMLDivElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    return toPagePoint(event.clientX - box.left, event.clientY - box.top, scale);
  };

  const end = (event: PointerEvent<HTMLDivElement>) => {
    if (!dragging.current) return;
    dragging.current = false;
    event.currentTarget.releasePointerCapture?.(event.pointerId);
    input?.finish();
  };

  return (
    <div
      className="text-layer"
      data-selectable={input ? 'true' : 'false'}
      onPointerDown={(event) => {
        if (!input || event.button !== 0 || !event.isPrimary) return;
        dragging.current = true;
        // Keep receiving moves when the pointer leaves the page mid-drag.
        event.currentTarget.setPointerCapture?.(event.pointerId);
        input.start(pageIndex, pointOf(event));
      }}
      onPointerMove={(event) => {
        if (dragging.current) input?.extend(pointOf(event));
      }}
      onPointerUp={end}
      onPointerCancel={end}
      onDoubleClick={(event) => input?.selectWord(pageIndex, pointOf(event))}
    >
      {hits?.map((match, index) => (
        <Highlights
          key={index}
          rects={match}
          scale={scale}
          className={match === currentHit ? 'search-hit search-hit--current' : 'search-hit'}
        />
      ))}
      {selection && <Highlights rects={selection} scale={scale} className="text-selection" />}
    </div>
  );
});

function Highlights({ rects, scale, className }: { rects: PageRects; scale: number; className: string }) {
  return toCssRects(rects, scale).map((rect, index) => (
    <div key={index} className={className} style={{ ...rect }} aria-hidden="true" />
  ));
}
