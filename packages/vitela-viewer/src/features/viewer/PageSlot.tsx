// One page's place in the viewer: sized and positioned by the layout,
// rendered at `dpi` while it is mounted, with the session's annotations over
// the bitmap, its text layer (search highlights, selection) on top and, over
// that, what makes the annotations selectable and editable — or, while a drawing tool is armed, the drawing layer over all of them. A
// PNG or JPEG dropped on the page is stamped where it was dropped. The
// viewer mounts only the slots near the viewport, so far-away pages hold no
// canvas, no annotation layer and no text layer at all. While the "Edit
// content" mode is on, its layer is over all of them and has the pointer.

import { memo, type DragEvent, type ReactNode } from 'react';
import type { Annotation, ContentItem, DocumentHandle, Engine, PageRect, PageRects, PageSize, Rgb } from '@vitela/core';
import { AnnotationEditLayer } from '../annotations/AnnotationEditLayer';
import { AnnotationLayer } from '../annotations/AnnotationLayer';
import { clampPoint, type ArmedTool, type Gesture } from '../annotations/drawing';
import { DrawingLayer } from '../annotations/DrawingLayer';
import { stampFileOf } from '../annotations/stamps';
import { useAnnotationDrag } from '../annotations/useAnnotationDrag';
import type { AnnotationInput } from '../annotations/useAnnotationEditing';
import type { ImageDrop } from '../annotations/useStamps';
import { AnnotationRing } from '../comments/AnnotationRing';
import { ContentLayer, type ContentInput } from '../content/ContentLayer';
import type { PageContentState } from '../content/contentModel';
import { PageCanvas } from '../render/PageCanvas';
import { usePageRender } from '../render/usePageRender';
import { toPagePoint } from '../text/pageRects';
import { TextLayer } from '../text/TextLayer';
import type { TextInput } from '../text/useTextSelection';
import type { Rect } from './layout';

interface PageSlotProps {
  readonly engine: Engine;
  readonly document: DocumentHandle;
  readonly index: number;
  /** The page as drawn: size in points and turn. */
  readonly page: PageSize;
  /** What is at this position (page id and turn): a change repaints it. */
  readonly pageKey: string;
  /** What pdfium draws of its form fields: a change repaints it, keeping the old bitmap meanwhile. */
  readonly paintKey?: string;
  /** This page's session annotations, in paint order. */
  readonly annotations: readonly Annotation[];
  /** The image of each stamp added this session, by annotation id. */
  readonly stampImages: ReadonlyMap<number, string> | undefined;
  readonly slot: Rect;
  readonly dpi: number;
  readonly label: string;
  /** CSS px per PDF point at the current zoom. */
  readonly scale: number;
  readonly hits?: readonly PageRects[];
  readonly currentHit?: PageRects;
  readonly selection?: PageRects;
  readonly textInput: TextInput | null;
  /** The armed drawing tool; `null` while Select is: the text layer has the pointer. */
  readonly drawing: DrawingInput | null;
  /** Selecting and editing annotations; `null` while a tool is armed or the document forbids it. */
  readonly editing: AnnotationInput | null;
  /** The selected annotation, when it is on this page. */
  readonly selectedId: number | null;
  /** What Previous/Next or the comments panel showed here, when it is not selected instead (drawn points). */
  readonly marked: PageRect | null;
  /** A PNG or JPEG dropped on the page, at the drop point (drawn points, kept on the page). */
  readonly onImageDrop: ImageDrop;
  /** The "Edit content" mode's pointer handling; `null` while the mode is off. */
  readonly content?: ContentInput | null;
  /** This page's parse in the mode. */
  readonly contentState?: PageContentState;
  /** The item under the pointer on this page, and the selected one when it is on this page. */
  readonly contentHovered?: ContentItem | null;
  readonly contentSelected?: ContentItem | null;
  /** The selected run's toolbar and editor, on its page only. */
  readonly contentOverlay?: ReactNode;
}

/** What the drawing layer needs from the viewer, the same for every page. */
export interface DrawingInput {
  readonly tool: ArmedTool;
  readonly color: Rgb;
  readonly busy: boolean;
  readonly onGesture: (pageIndex: number, pageKey: string, tool: ArmedTool, gesture: Gesture) => void;
}

export const PageSlot = memo(function PageSlot(props: PageSlotProps) {
  const { engine, document, index, page, pageKey, paintKey, annotations, stampImages, slot, dpi, label, scale } = props;
  const { hits, currentHit, selection, textInput, drawing, editing, selectedId, marked, onImageDrop } = props;
  const { content, contentState, contentHovered = null, contentSelected = null, contentOverlay = null } = props;
  const render = usePageRender(engine, document, index, dpi, pageKey, paintKey);
  const { shown: shownAnnotations, drag } = useAnnotationDrag(annotations, page, pageKey, scale, editing);
  const shown = render.status === 'rendered' ? render.page : render.status === 'rendering' ? render.previous : undefined;
  // An image dropped here is stamped, not opened: claiming the drop
  // (`preventDefault`) tells the workspace's window-wide listener, which sees
  // it next, to open nothing. A drop with a PDF in it is still the workspace's.
  // The drop point is kept on the page, as a click's is (`finishGesture`).
  const dropped = (event: DragEvent<HTMLDivElement>) => {
    const file = stampFileOf([...(event.dataTransfer?.files ?? [])]);
    if (!file) return;
    event.preventDefault();
    const box = event.currentTarget.getBoundingClientRect();
    const point = toPagePoint(event.clientX - box.left, event.clientY - box.top, scale);
    onImageDrop(index, pageKey, clampPoint(point, page), file);
  };
  return (
    <div
      className="page-slot"
      style={{ top: slot.top, left: slot.left, width: slot.width, height: slot.height }}
      // E2E waits on these: which page, and whether its bitmap is current.
      data-page={index + 1}
      data-state={render.status}
      data-dpi={dpi}
      onDrop={dropped}
    >
      {shown && <PageCanvas page={shown} label={label} />}
      <AnnotationLayer page={page} annotations={shownAnnotations} stampImages={stampImages} />
      <TextLayer
        pageIndex={index}
        scale={scale}
        hits={hits}
        currentHit={currentHit}
        selection={selection}
        input={drawing ? null : textInput}
      />
      {editing && shownAnnotations.length > 0 && (
        <AnnotationEditLayer
          annotations={shownAnnotations}
          pageKey={pageKey}
          scale={scale}
          selectedId={selectedId}
          input={editing}
          drag={drag}
        />
      )}
      {marked && <AnnotationRing rect={marked} scale={scale} />}
      {drawing && (
        <DrawingLayer
          pageIndex={index}
          page={page}
          pageKey={pageKey}
          scale={scale}
          tool={drawing.tool}
          color={drawing.color}
          busy={drawing.busy}
          onGesture={drawing.onGesture}
        />
      )}
      {content && (
        <ContentLayer
          pageIndex={index}
          scale={scale}
          state={contentState}
          hovered={contentHovered}
          selected={contentSelected}
          input={content}
        >
          {contentOverlay}
        </ContentLayer>
      )}
      {render.status === 'failed' && (
        <p className="page-slot__error" role="alert">{`Could not render this page: ${render.message}`}</p>
      )}
    </div>
  );
});
