// Zoom state of the viewer: buttons and fit width zoom around the viewport
// centre, Ctrl/⌘+wheel zooms around the pointer, and a plain wheel is left
// to scroll. The anchoring math is `zoomAround` (zoom.ts); this hook only
// applies its scroll once React has laid the new zoom out.
//
// Until then it reports that scroll as `pendingScroll`, so the viewer can
// lay out the zoom's commit at the scroll it is about to get. Call it before
// any hook that reads the scroller's scroll in a layout effect: its own
// layout effect must write the scroll first.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import type { PageSize } from '../../core/types';
import type { Point } from './layout';
import { fitWidthZoom, wheelZoom, zoomAround } from './zoom';

interface ZoomState {
  readonly zoom: number;
  /** The scroll this zoom still has to apply; `null` once applied. */
  readonly scroll: Point | null;
}

export function useViewerZoom(scroller: RefObject<HTMLElement | null>, pages: readonly PageSize[]) {
  // A fresh object per request, even when the zoom does not change (Fit
  // width twice, Ctrl+wheel at a limit): React then always commits it and
  // the layout effect below always applies, and clears, its scroll.
  const [view, setView] = useState<ZoomState>({ zoom: 1, scroll: null });
  const { zoom } = view;
  // Wheel events can arrive faster than React commits: each one builds on
  // the zoom and scroll the previous one asked for, not on stale DOM values.
  const target = useRef(1);
  const pendingScroll = useRef<Point | null>(null);

  const zoomTo = useCallback(
    (next: number, pointer?: Point) => {
      const element = scroller.current;
      if (!element) return;
      const viewport = { width: element.clientWidth, height: element.clientHeight };
      const view = {
        zoom: target.current,
        scroll: pendingScroll.current ?? { x: element.scrollLeft, y: element.scrollTop },
      };
      const centre = { x: viewport.width / 2, y: viewport.height / 2 };
      const result = zoomAround(pages, viewport, view, next, pointer ?? centre);
      target.current = result.zoom;
      pendingScroll.current = result.scroll;
      setView(result);
    },
    [scroller, pages],
  );

  // The content has its new size once React commits: scroll then, before paint.
  useLayoutEffect(() => {
    const element = scroller.current;
    const scroll = pendingScroll.current;
    if (!element || !scroll) return;
    pendingScroll.current = null;
    element.scrollLeft = scroll.x;
    element.scrollTop = scroll.y;
    // Re-renders before paint, together with the scroll metrics re-read.
    setView((current) => (current === view ? { zoom: current.zoom, scroll: null } : current));
  }, [scroller, view]);

  useEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) return;
      // Not passive: this stops the browser zooming the whole page.
      event.preventDefault();
      const box = element.getBoundingClientRect();
      const pointer = {
        x: event.clientX - box.left - element.clientLeft,
        y: event.clientY - box.top - element.clientTop,
      };
      zoomTo(wheelZoom(target.current, event.deltaY, event.deltaMode), pointer);
    };
    element.addEventListener('wheel', wheel, { passive: false });
    return () => element.removeEventListener('wheel', wheel);
  }, [scroller, zoomTo]);

  const fitWidth = useCallback(() => {
    const element = scroller.current;
    if (element) zoomTo(fitWidthZoom(pages, element.clientWidth));
  }, [scroller, pages, zoomTo]);

  return { zoom, pendingScroll: view.scroll, zoomTo, fitWidth };
}
