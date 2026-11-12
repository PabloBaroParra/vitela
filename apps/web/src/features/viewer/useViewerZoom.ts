// Zoom state of the viewer: buttons and fit width zoom around the viewport
// centre, Ctrl/⌘+wheel zooms around the pointer, and a plain wheel is left
// to scroll. The anchoring math is `zoomAround` (zoom.ts); this hook only
// applies its scroll once React has laid the new zoom out.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import type { PageSize } from '../../core/types';
import type { Point } from './layout';
import { fitWidthZoom, wheelZoom, zoomAround } from './zoom';

export function useViewerZoom(scroller: RefObject<HTMLElement | null>, pages: readonly PageSize[]) {
  const [zoom, setZoom] = useState(1);
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
      setZoom(result.zoom);
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
  }, [scroller, zoom]);

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

  return { zoom, zoomTo, fitWidth };
}
