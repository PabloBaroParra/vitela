import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { PageSize } from '../../core/types';
import { useViewerZoom } from './useViewerZoom';
import { MAX_ZOOM, zoomAround } from './zoom';

const pages: PageSize[] = Array(10).fill({ widthPt: 612, heightPt: 792, rotationDegrees: 0 });
const viewport = { width: 800, height: 600 };
const centre = { x: viewport.width / 2, y: viewport.height / 2 };

/** A scroller with a fixed viewport whose scroll offsets tests can set. */
function scroller(): HTMLDivElement {
  const element = document.createElement('div');
  Object.defineProperties(element, {
    clientWidth: { value: viewport.width },
    clientHeight: { value: viewport.height },
    scrollTop: { value: 0, writable: true },
    scrollLeft: { value: 0, writable: true },
  });
  return element;
}

describe('useViewerZoom', () => {
  it('anchors the next zoom on the current scroll after a zoom that changed nothing', () => {
    const element = scroller();
    const { result } = renderHook(() => useViewerZoom({ current: element }, pages));

    // Fit width twice, or Ctrl+wheel at a limit: the zoom does not change.
    act(() => result.current.zoomTo(1));
    // The user scrolls down, then zooms.
    element.scrollTop = 3000;
    element.scrollLeft = 20;
    act(() => result.current.zoomTo(2));

    const expected = zoomAround(pages, viewport, { zoom: 1, scroll: { x: 20, y: 3000 } }, 2, centre).scroll;
    expect(result.current.zoom).toBe(2);
    expect({ x: element.scrollLeft, y: element.scrollTop }).toEqual(expected);
  });

  it('anchors on the current scroll after a wheel zoom past the maximum', () => {
    const element = scroller();
    const { result } = renderHook(() => useViewerZoom({ current: element }, pages));
    act(() => result.current.zoomTo(MAX_ZOOM));
    act(() => result.current.zoomTo(MAX_ZOOM * 2));
    element.scrollTop = 9000;
    element.scrollLeft = 0;
    act(() => result.current.zoomTo(2));

    const expected = zoomAround(pages, viewport, { zoom: MAX_ZOOM, scroll: { x: 0, y: 9000 } }, 2, centre).scroll;
    expect({ x: element.scrollLeft, y: element.scrollTop }).toEqual(expected);
  });
});
