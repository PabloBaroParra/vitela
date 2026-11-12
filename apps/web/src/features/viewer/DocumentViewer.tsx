// Container: the open document as one vertically scrolling, virtualized
// column of pages, with zoom, search and text selection. Only pages within a
// viewport of the visible area are mounted (and so rendered); the rest are
// empty space.

import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { Engine } from '../../core/types';
import type { OpenDocument } from '../document/useDocument';
import { currentHit, hitsByPage, hitsOf } from '../search/searchState';
import { useSearch } from '../search/useSearch';
import { boundingBox, toCssRects } from '../text/pageRects';
import { TextToolbar } from '../text/TextToolbar';
import { useCopy } from '../text/useCopy';
import { useTextSelection } from '../text/useTextSelection';
import { CSS_PX_PER_PT, layoutPages, pageAt, visiblePages } from './layout';
import { PageSlot } from './PageSlot';
import { renderDpi } from './renderDpi';
import { revealScroll } from './reveal';
import { useScrollMetrics } from './useScrollMetrics';
import { useSettled } from './useSettled';
import { useViewerZoom } from './useViewerZoom';
import { MAX_ZOOM, MIN_ZOOM, zoomIn, zoomOut } from './zoom';
import { ZoomControls } from './ZoomControls';

/** How long a zoom must hold still before pages re-render at its DPI. */
const RENDER_SETTLE_MS = 150;
/** Room kept around a search match scrolled into view, in CSS px. */
const REVEAL_MARGIN = 48;

interface DocumentViewerProps {
  readonly engine: Engine;
  readonly document: OpenDocument;
}

export function DocumentViewer({ engine, document }: DocumentViewerProps) {
  const scroller = useRef<HTMLDivElement>(null);
  const { pages } = document.info;
  // Zoom first: its layout effect must scroll before the metrics re-read.
  const { zoom, pendingScroll, zoomTo, fitWidth } = useViewerZoom(scroller, pages);
  const metrics = useScrollMetrics(scroller);
  const renderZoom = useSettled(zoom, RENDER_SETTLE_MS);
  const pixelRatio = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
  const scale = CSS_PX_PER_PT * zoom;

  // Known at open; an engine refusal (`TextExtractionDenied`) also turns
  // every text feature off, whatever the open said.
  const [textAllowed, setTextAllowed] = useState(document.info.textExtractionAllowed);
  const denyText = useCallback(() => setTextAllowed(false), []);
  const search = useSearch(engine, document.handle, denyText);
  const { selection, input, clear: clearSelection } = useTextSelection(engine, document.handle, denyText);
  const copy = useCopy(textAllowed ? (selection?.text ?? null) : null);

  const layout = useMemo(() => layoutPages(pages, zoom, metrics.width), [pages, zoom, metrics.width]);
  // On a zoom's commit the scroller still has the old scroll: lay out at the
  // one the zoom is about to apply, or the pages in view would unmount.
  const top = pendingScroll?.y ?? metrics.top;
  // One viewport of overscan on each side keeps short scrolls painted.
  const range = visiblePages(layout, top, metrics.height, metrics.height);
  const current = (pageAt(layout, top + metrics.height / 2) ?? 0) + 1;

  const hits = hitsOf(search.state);
  const pageHits = useMemo(() => hitsByPage(hits), [hits]);
  const shownHit = currentHit(search.state);

  // Every move between matches (a new search state) scrolls its match into
  // view — from the layout, so a page that is not mounted yet works too.
  // Read through a ref: a zoom must not scroll back to the match.
  const view = useRef({ layout, scale });
  useLayoutEffect(() => {
    view.current = { layout, scale };
  });
  useLayoutEffect(() => {
    const element = scroller.current;
    const hit = currentHit(search.state);
    if (!element || !hit) return;
    const { layout, scale } = view.current;
    const slot = layout.slots[hit.pageIndex];
    const box = slot && boundingBox(toCssRects(hit.rects, scale));
    if (!slot || !box) return;
    const target = { ...box, left: slot.left + box.left, top: slot.top + box.top };
    const viewport = {
      left: element.scrollLeft,
      top: element.scrollTop,
      width: element.clientWidth,
      height: element.clientHeight,
    };
    const scroll = revealScroll(viewport, target, REVEAL_MARGIN);
    if (!scroll) return;
    element.scrollLeft = scroll.x;
    element.scrollTop = scroll.y;
  }, [search.state]);

  const slots = [];
  if (range && metrics.height > 0) {
    for (let index = range.first; index <= range.last; index++) {
      slots.push(
        <PageSlot
          key={index}
          engine={engine}
          document={document.handle}
          index={index}
          slot={layout.slots[index]!}
          dpi={renderDpi(pages[index]!, renderZoom, pixelRatio)}
          label={`Page ${index + 1} of ${document.name}`}
          scale={scale}
          hits={textAllowed ? pageHits.get(index) : undefined}
          currentHit={textAllowed && shownHit?.pageIndex === index ? shownHit.rects : undefined}
          selection={textAllowed && selection?.pageIndex === index ? selection.rects : undefined}
          textInput={textAllowed ? input : null}
        />,
      );
    }
  }

  return (
    <section className="viewer" aria-label={document.name}>
      <div className="viewer__toolbars">
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
        <TextToolbar
          allowed={textAllowed}
          search={search.state}
          onSubmit={search.submit}
          onNext={search.next}
          onPrevious={search.previous}
          onClear={search.clear}
          canCopy={!!selection}
          onCopy={copy.copy}
          copyStatus={copy.status}
        />
      </div>
      {/* Focusable so the keyboard can scroll it (and Escape drops a selection). */}
      <div
        ref={scroller}
        className="viewer__scroller"
        tabIndex={0}
        aria-label="Pages"
        data-zoom={zoom}
        onKeyDown={(event) => {
          if (event.key === 'Escape') clearSelection();
        }}
      >
        <div className="viewer__content" style={{ width: layout.width, height: layout.height }}>
          {slots}
        </div>
      </div>
    </section>
  );
}
