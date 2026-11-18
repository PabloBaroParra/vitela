// One page's place in the viewer: sized and positioned by the layout,
// rendered at `dpi` while it is mounted, with the session's annotations over
// the bitmap and its text layer (search highlights, selection) on top —
// or, while a drawing tool is armed, the drawing layer over all of them. The
// viewer mounts only the slots near the viewport, so far-away pages hold no
// canvas, no annotation layer and no text layer at all.

import { memo } from 'react';
import type { Annotation, DocumentHandle, Engine, PageRects, PageSize, Rgb } from '@vitela/core';
import { AnnotationLayer } from '../annotations/AnnotationLayer';
import type { ArmedTool, Gesture } from '../annotations/drawing';
import { DrawingLayer } from '../annotations/DrawingLayer';
import { PageCanvas } from '../render/PageCanvas';
import { usePageRender } from '../render/usePageRender';
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
  /** This page's session annotations, in paint order. */
  readonly annotations: readonly Annotation[];
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
}

/** What the drawing layer needs from the viewer, the same for every page. */
export interface DrawingInput {
  readonly tool: ArmedTool;
  readonly color: Rgb;
  readonly busy: boolean;
  readonly onGesture: (pageIndex: number, pageKey: string, tool: ArmedTool, gesture: Gesture) => void;
}

export const PageSlot = memo(function PageSlot(props: PageSlotProps) {
  const { engine, document, index, page, pageKey, annotations, slot, dpi, label, scale } = props;
  const { hits, currentHit, selection, textInput, drawing } = props;
  const render = usePageRender(engine, document, index, dpi, pageKey);
  const shown = render.status === 'rendered' ? render.page : render.status === 'rendering' ? render.previous : undefined;
  return (
    <div
      className="page-slot"
      style={{ top: slot.top, left: slot.left, width: slot.width, height: slot.height }}
      // E2E waits on these: which page, and whether its bitmap is current.
      data-page={index + 1}
      data-state={render.status}
      data-dpi={dpi}
    >
      {shown && <PageCanvas page={shown} label={label} />}
      <AnnotationLayer page={page} annotations={annotations} />
      <TextLayer
        pageIndex={index}
        scale={scale}
        hits={hits}
        currentHit={currentHit}
        selection={selection}
        input={drawing ? null : textInput}
      />
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
      {render.status === 'failed' && (
        <p className="page-slot__error" role="alert">{`Could not render this page: ${render.message}`}</p>
      )}
    </div>
  );
});
