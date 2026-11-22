// Container: the open document as one vertically scrolling, virtualized
// column of pages, with zoom, search, text selection, the session's
// annotations, markup over the selection, the drawing tools, image stamps
// (picked or dropped on a page) and undo/redo. Only pages within a
// viewport of the visible area are mounted (and so rendered); the rest are
// empty space.

import { useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { Annotation, DocumentHandle, EditedDocument, Engine, MarkupKind } from '@vitela/core';
import { DraftDialog } from '../annotations/DraftDialog';
import { DrawToolbar } from '../annotations/DrawToolbar';
import { pageKey, useAnnotationEdits } from '../annotations/useAnnotationEdits';
import { useDrawing } from '../annotations/useDrawing';
import { useStamps } from '../annotations/useStamps';
import { EditorScopeContext } from '../document/editorScope';
import type { OpenDocument } from '../document/useDocument';
import { useMarkDirty } from '../save/MarkDirtyContext';
import { currentHit, hitsByPage, hitsOf } from '../search/searchState';
import { useSearch } from '../search/useSearch';
import { boundingBox, toCssRects } from '../text/pageRects';
import { TextToolbar } from '../text/TextToolbar';
import { useCopy } from '../text/useCopy';
import { useTextSelection } from '../text/useTextSelection';
import { HistoryControls } from './HistoryControls';
import { CSS_PX_PER_PT, layoutPages, pageAt, visiblePages } from './layout';
import { PageSlot, type DrawingInput } from './PageSlot';
import { renderDpi } from './renderDpi';
import { revealScroll } from './reveal';
import { useScrollMetrics } from './useScrollMetrics';
import { useSettled } from './useSettled';
import { useUndoKeys } from './useUndoKeys';
import { useViewerZoom } from './useViewerZoom';
import { MAX_ZOOM, MIN_ZOOM, zoomIn, zoomOut } from './zoom';
import { ZoomControls } from './ZoomControls';

/** How long a zoom must hold still before pages re-render at its DPI. */
const RENDER_SETTLE_MS = 150;
/** Room kept around a search match scrolled into view, in CSS px. */
const REVEAL_MARGIN = 48;

const NO_ANNOTATIONS: readonly Annotation[] = [];

interface DocumentViewerProps {
  readonly engine: Engine;
  readonly document: OpenDocument;
  /**
   * Hands the edited document to the workspace; `false` when that document
   * is no longer the one on screen (then it is not marked dirty either).
   */
  readonly onEdited: (handle: DocumentHandle, edited: EditedDocument) => boolean;
  /**
   * Keeps the image of stamp `id` with the document (the engine hands none
   * back), so it is drawn again after a redo or when the viewer remounts.
   */
  readonly onStampImage: (handle: DocumentHandle, id: number, href: string) => void;
}

export function DocumentViewer({ engine, document, onEdited, onStampImage }: DocumentViewerProps) {
  const scroller = useRef<HTMLDivElement>(null);
  const { pages, pageIds } = document.info;
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
  // What sits at each position: an undo or redo of a page edit (made in
  // Organize) puts other pages there, so their bitmaps, search matches and
  // the selection no longer fit.
  const pageKeys = useMemo(() => pages.map((_, index) => pageKey({ pages, pageIds }, index)), [pages, pageIds]);
  // Read through a ref: a selection records the page on screen at its press.
  const shownKeys = useRef(pageKeys);
  useLayoutEffect(() => {
    shownKeys.current = pageKeys;
  });
  const keyOf = useCallback((index: number) => shownKeys.current[index] ?? String(index), []);
  const { selection, input, clear: clearSelection } = useTextSelection(engine, document.handle, denyText, keyOf);
  const scope = useContext(EditorScopeContext);
  const copy = useCopy(textAllowed ? (selection?.text ?? null) : null, scope);

  const markDirty = useMarkDirty();
  const { handle } = document;
  const report = useCallback(
    (edited: EditedDocument) => {
      if (onEdited(handle, edited)) markDirty();
    },
    [onEdited, handle, markDirty],
  );
  const editing = useAnnotationEdits(engine, document, report);
  useUndoKeys(editing.undo, editing.redo, scope);
  // Read through a ref: a markup's end must see the selection shown then.
  const shownSelection = useRef(selection);
  useLayoutEffect(() => {
    shownSelection.current = selection;
  });
  // Off while any step runs: an undo may be putting another page here.
  const canMark = !!selection && !editing.pending;

  // An armed tool takes the pointer from text selection, and drops the selection.
  const shownPages = useRef(pages);
  useLayoutEffect(() => {
    shownPages.current = pages;
  });
  const rotationOf = useCallback((index: number) => shownPages.current[index]?.rotationDegrees ?? 0, []);
  const keepStampImage = useCallback((id: number, href: string) => onStampImage(handle, id, href), [onStampImage, handle]);
  const stamps = useStamps(engine, handle, editing, keepStampImage);
  const drawing = useDrawing(editing, rotationOf, stamps, scope);
  const armed = drawing.tool === 'select' ? null : drawing.tool;
  useEffect(() => {
    if (armed) clearSelection();
  }, [armed, clearSelection]);
  const drawingInput = useMemo<DrawingInput | null>(
    () => armed && { tool: armed, color: drawing.color, busy: editing.pending, onGesture: drawing.onGesture },
    [armed, drawing.color, editing.pending, drawing.onGesture],
  );

  // A new page list drops the selection once committed; until then a markup
  // checks the page the selection was made on (its `pageKey`).
  const structure = pageKeys.join(',');
  const shownStructure = useRef(structure);
  const clearSearch = search.clear;
  useEffect(() => {
    if (shownStructure.current === structure) return;
    shownStructure.current = structure;
    clearSelection();
    clearSearch();
  }, [structure, clearSelection, clearSearch]);

  const markSelection = (kind: MarkupKind) => {
    const marked = selection;
    if (!marked || !canMark) return;
    // Checked against the page the text was selected on, not the one there
    // now. Once any line went in, the selection goes (marking it again would
    // repeat those lines); it stays when nothing was marked, or when another
    // selection was made since.
    void editing.markup(kind, marked.pageIndex, marked.pageKey, marked.rects).then((outcome) => {
      const applied = outcome === 'marked' || outcome === 'partial';
      if (applied && shownSelection.current === marked) clearSelection();
    });
  };

  const annotations = document.annotations ?? NO_ANNOTATIONS;
  const annotationsByPage = useMemo(() => {
    const byPage = new Map<number, Annotation[]>();
    for (const annotation of annotations) {
      const list = byPage.get(annotation.pageIndex);
      if (list) list.push(annotation);
      else byPage.set(annotation.pageIndex, [annotation]);
    }
    return byPage;
  }, [annotations]);

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
          page={pages[index]!}
          pageKey={pageKeys[index]!}
          annotations={annotationsByPage.get(index) ?? NO_ANNOTATIONS}
          stampImages={document.stampImages}
          slot={layout.slots[index]!}
          dpi={renderDpi(pages[index]!, renderZoom, pixelRatio)}
          label={`Page ${index + 1} of ${document.name}`}
          scale={scale}
          hits={textAllowed ? pageHits.get(index) : undefined}
          currentHit={textAllowed && shownHit?.pageIndex === index ? shownHit.rects : undefined}
          selection={textAllowed && selection?.pageIndex === index ? selection.rects : undefined}
          textInput={textAllowed ? input : null}
          drawing={drawingInput}
          onImageDrop={stamps.drop}
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
          markup={{ allowed: editing.allowed, canMark, onMark: markSelection }}
        />
        <DrawToolbar
          tool={drawing.tool}
          onTool={drawing.choose}
          color={drawing.color}
          onColor={drawing.setColor}
          onImage={drawing.pickImage}
          allowed={editing.allowed}
          ready={!editing.pending}
        />
        <HistoryControls
          canUndo={editing.canUndo}
          canRedo={editing.canRedo}
          onUndo={editing.undo}
          onRedo={editing.redo}
        />
        {editing.failure && (
          <p className="viewer__failure" role="alert">
            {editing.failure}
          </p>
        )}
      </div>
      {/* Focusable so the keyboard can scroll it (and Escape drops a selection). */}
      <div
        ref={scroller}
        className="viewer__scroller"
        tabIndex={0}
        aria-label="Pages"
        data-zoom={zoom}
        data-tool={drawing.tool}
        onKeyDown={(event) => {
          if (event.key === 'Escape') clearSelection();
        }}
      >
        <div className="viewer__content" style={{ width: layout.width, height: layout.height }}>
          {slots}
        </div>
      </div>
      {drawing.draft && (
        <DraftDialog
          engine={engine}
          draft={drawing.draft}
          onChange={drawing.retyped}
          onSubmit={(text) => void drawing.submit(text)}
          onCancel={drawing.cancel}
        />
      )}
    </section>
  );
}
