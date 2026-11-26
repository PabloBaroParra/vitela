// Container: the open document as one vertically scrolling, virtualized
// column of pages, with zoom, search, text selection, the session's
// annotations, markup over the selection, the drawing tools, image stamps
// (picked or dropped on a page), selecting and editing an annotation, going
// from one annotation to the next, the comments, filling in form fields, and
// undo/redo. Only pages within a viewport of the visible area are mounted (and
// so rendered); the rest are empty space.

import { useCallback, useContext, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { Annotation, DocumentHandle, EditedDocument, Engine, MarkupKind } from '@vitela/core';
import { AnnotationDialog } from '../annotations/AnnotationDialog';
import { AnnotationToolbar } from '../annotations/AnnotationToolbar';
import { DraftDialog } from '../annotations/DraftDialog';
import { DrawToolbar } from '../annotations/DrawToolbar';
import { recolor } from '../annotations/editBuilds';
import { useAnnotationEditing } from '../annotations/useAnnotationEditing';
import { pageKey, useAnnotationEdits } from '../annotations/useAnnotationEdits';
import { useDrawing } from '../annotations/useDrawing';
import { useStamps } from '../annotations/useStamps';
import { CommentsPanel } from '../comments/CommentsPanel';
import { NavigationToolbar } from '../comments/NavigationToolbar';
import { NoteDialog, type ReadableNote } from '../comments/NoteDialog';
import { commentEntries } from '../comments/stops';
import { useAnnotationNavigation } from '../comments/useAnnotationNavigation';
import { useComments } from '../comments/useComments';
import { EditorScopeContext } from '../document/editorScope';
import type { OpenDocument } from '../document/useDocument';
import { FormsPanel } from '../forms/FormsPanel';
import { FormsToggle } from '../forms/FormsToggle';
import { useForms } from '../forms/useForms';
import { useMarkDirty, useSaveWaitsFor } from '../save/MarkDirtyContext';
import { currentHit, hitsByPage, hitsOf } from '../search/searchState';
import { useSearch } from '../search/useSearch';
import { TextToolbar } from '../text/TextToolbar';
import { useCopy } from '../text/useCopy';
import { useTextSelection } from '../text/useTextSelection';
import { HistoryControls } from './HistoryControls';
import { CSS_PX_PER_PT, layoutPages, pageAt, visiblePages } from './layout';
import { PageSlot, type DrawingInput } from './PageSlot';
import { renderDpi } from './renderDpi';
import { useReveal } from './useReveal';
import { useRevealMatch } from './useRevealMatch';
import { useScrollMetrics } from './useScrollMetrics';
import { useSettled } from './useSettled';
import { useUndoKeys } from './useUndoKeys';
import { useViewerZoom } from './useViewerZoom';
import { MAX_ZOOM, MIN_ZOOM, zoomIn, zoomOut } from './zoom';
import { ZoomControls } from './ZoomControls';

/** How long a zoom must hold still before pages re-render at its DPI. */
const RENDER_SETTLE_MS = 150;

/** The annotation toolbar over the foot of the pages, at two rows, in CSS px. */
const ANNOTATION_TOOLBAR_ROOM = 112;

const NO_ANNOTATIONS: readonly Annotation[] = [];

/** What sits beside the pages, one at a time. */
type SidePanel = 'comments' | 'forms' | null;

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
  useSaveWaitsFor(editing.settled);
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

  // Select also picks annotations: one selection at a time, text or annotation.
  const annotations = document.annotations ?? NO_ANNOTATIONS;
  const focusPages = useCallback(() => scroller.current?.focus({ preventScroll: true }), []);
  const annotating = useAnnotationEditing(editing, annotations, keyOf, editing.allowed && !armed, clearSelection, focusPages);
  const { selected } = annotating;

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

  const annotationToolbar = useRef<HTMLDivElement>(null);
  const reveal = useReveal(scroller, layout, scale);
  useRevealMatch(reveal, search.state);

  // Reading what is annotated: nothing here depends on what may be edited.
  // Every edited document brings a new `info`, also when only an annotation changed.
  const comments = useComments(engine, handle, document.info, structure);
  const entries = useMemo(
    () => commentEntries(comments.state.comments, (index) => pageKeys[index] ?? String(index)),
    [comments.state.comments, pageKeys],
  );
  const navigation = useAnnotationNavigation(
    pages,
    annotations,
    entries,
    selected,
    annotating.input,
    annotating.deselect,
    reveal,
    annotationToolbar,
  );
  const { marked } = navigation;
  const [panel, setPanel] = useState<SidePanel>(null);
  const commentsShown = panel === 'comments';
  const formsShown = panel === 'forms';
  const togglePanel = (which: Exclude<SidePanel, null>) => setPanel((shown) => (shown === which ? null : which));
  const [reading, setReading] = useState<ReadableNote | null>(null);
  const commentsToggle = useRef<HTMLButtonElement>(null);
  const commentsId = useId();
  const closeComments = () => {
    setPanel(null);
    commentsToggle.current?.focus({ preventScroll: true });
  };

  const forms = useForms(engine, document, editing, reveal, annotationToolbar);
  const formsToggle = useRef<HTMLButtonElement>(null);
  const formsId = useId();
  const closeForms = () => {
    setPanel(null);
    forms.blurField();
    formsToggle.current?.focus({ preventScroll: true });
  };
  // A field focused in the panel is ringed on its page, unless an annotation already is.
  const ringed = marked ?? (formsShown ? forms.spot : null);

  // Always there while annotations can be selected (so selecting moves nothing):
  // the last page's foot can be scrolled above the annotation toolbar.
  const footRoom = editing.allowed && annotations.length > 0 ? ANNOTATION_TOOLBAR_ROOM : 0;

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
          editing={annotating.input}
          selectedId={selected?.pageIndex === index ? selected.id : null}
          marked={ringed?.pageIndex === index ? ringed.rect : null}
          paintKey={forms.paintKeys.get(index) ?? ''}
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
        <NavigationToolbar
          stops={navigation.stops}
          current={navigation.current}
          onStep={navigation.step}
          commentsToggle={commentsToggle}
          commentsId={commentsId}
          commentsShown={commentsShown}
          commentsFailed={comments.state.status === 'failed'}
          onComments={() => togglePanel('comments')}
        >
          <FormsToggle toggle={formsToggle} panelId={formsId} shown={formsShown} onToggle={() => togglePanel('forms')} />
        </NavigationToolbar>
        {editing.failure && (
          <p className="viewer__failure" role="alert">
            {editing.failure}
          </p>
        )}
      </div>
      <div className="viewer__body">
        <div className="viewer__pages">
          {/* Focusable so the keyboard can scroll it (and Escape drops a selection). */}
          <div
            ref={scroller}
            className="viewer__scroller"
            tabIndex={0}
            aria-label="Pages"
            data-zoom={zoom}
            data-tool={drawing.tool}
            onKeyDown={(event) => {
              if (event.key !== 'Escape') return;
              clearSelection();
              navigation.unmark();
            }}
            // A press beside the selected annotation lets it go (and goes on to select text).
            onPointerDown={(event) => {
              const pressed = event.target as Element;
              const onPage = event.button === 0 && event.isPrimary && pressed !== event.currentTarget;
              if (onPage && !pressed.closest('.annotation-edit-layer')) annotating.deselect();
              if (onPage) navigation.unmark();
            }}
          >
            <div className="viewer__content" style={{ width: layout.width, height: layout.height + footRoom }}>
              {slots}
            </div>
          </div>
          {selected && (
            <AnnotationToolbar
              toolbar={annotationToolbar}
              annotation={selected}
              onDialog={annotating.openDialog}
              onColor={(color) => annotating.changeSelected(recolor(color))}
              onRead={() => selected.kind === 'note' && setReading({ ...selected, author: null, date: null })}
              onRemove={annotating.removeSelected}
              onFocusLost={focusPages}
            />
          )}
        </div>
        {commentsShown && (
          <CommentsPanel
            id={commentsId}
            state={comments.state}
            entries={entries}
            currentKey={navigation.current?.key ?? null}
            onGo={navigation.visitKey}
            onRead={setReading}
            onRetry={comments.retry}
            onClose={closeComments}
          />
        )}
        {formsShown && (
          <FormsPanel
            id={formsId}
            state={forms.state}
            allowed={forms.allowed}
            errors={forms.errors}
            onFill={forms.fill}
            onFocusField={forms.focusField}
            onBlurField={forms.blurField}
            onRetry={forms.retry}
            onClose={closeForms}
          />
        )}
      </div>
      {selected && annotating.dialog && (
        <AnnotationDialog
          engine={engine}
          dialog={annotating.dialog}
          annotation={selected}
          page={pages[selected.pageIndex]!}
          onChange={annotating.dialogEdited}
          onSubmit={(build) => void annotating.submitDialog(build)}
          onCancel={annotating.cancelDialog}
        />
      )}
      {reading && <NoteDialog note={reading} onClose={() => setReading(null)} />}
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
