// Container: the open document as one vertically scrolling, virtualized
// column of pages, with zoom. Only pages within a viewport of the visible
// area are mounted (and so rendered); the rest are empty space.

import { useMemo, useRef } from 'react';
import type { Engine } from '../../core/types';
import type { OpenDocument } from '../document/useDocument';
import { layoutPages, pageAt, visiblePages } from './layout';
import { PageSlot } from './PageSlot';
import { renderDpi } from './renderDpi';
import { useScrollMetrics } from './useScrollMetrics';
import { useSettled } from './useSettled';
import { useViewerZoom } from './useViewerZoom';
import { MAX_ZOOM, MIN_ZOOM, zoomIn, zoomOut } from './zoom';
import { ZoomControls } from './ZoomControls';

/** How long a zoom must hold still before pages re-render at its DPI. */
const RENDER_SETTLE_MS = 150;

interface DocumentViewerProps {
  readonly engine: Engine;
  readonly document: OpenDocument;
}

export function DocumentViewer({ engine, document }: DocumentViewerProps) {
  const scroller = useRef<HTMLDivElement>(null);
  const { pages } = document.info;
  const metrics = useScrollMetrics(scroller);
  const { zoom, zoomTo, fitWidth } = useViewerZoom(scroller, pages);
  const renderZoom = useSettled(zoom, RENDER_SETTLE_MS);
  const pixelRatio = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;

  const layout = useMemo(() => layoutPages(pages, zoom, metrics.width), [pages, zoom, metrics.width]);
  // One viewport of overscan on each side keeps short scrolls painted.
  const range = visiblePages(layout, metrics.top, metrics.height, metrics.height);
  const current = (pageAt(layout, metrics.top + metrics.height / 2) ?? 0) + 1;

  const slots = [];
  if (range && metrics.height > 0) {
    for (let index = range.first; index <= range.last; index++) {
      slots.push(
        <PageSlot
          key={index}
          engine={engine}
          generation={document.generation}
          index={index}
          slot={layout.slots[index]!}
          dpi={renderDpi(pages[index]!, renderZoom, pixelRatio)}
          label={`Page ${index + 1} of ${document.name}`}
        />,
      );
    }
  }

  return (
    <section className="viewer" aria-label={document.name}>
      <ZoomControls
        zoom={zoom}
        page={current}
        pageCount={pages.length}
        onZoomIn={() => zoomTo(zoomIn(zoom))}
        onZoomOut={() => zoomTo(zoomOut(zoom))}
        onFitWidth={fitWidth}
        canZoomIn={zoom < MAX_ZOOM}
        canZoomOut={zoom > MIN_ZOOM}
      />
      {/* Focusable so the keyboard can scroll it. */}
      <div ref={scroller} className="viewer__scroller" tabIndex={0} aria-label="Pages" data-zoom={zoom}>
        <div className="viewer__content" style={{ width: layout.width, height: layout.height }}>
          {slots}
        </div>
      </div>
    </section>
  );
}
