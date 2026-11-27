// One page's layer of the "Edit content" mode (B26 T-258b): over everything
// else on the page while the mode is on, it takes the pointer, outlines the
// item under it and the one selected — a text run, an image or a shape, each
// in its own style — and reports a press. Presentational: what is under a
// point is the container's to decide.

import type { ContentItem, PagePoint } from '@vitela/core';
import type { PointerEvent } from 'react';
import { toPagePoint } from '../text/pageRects';
import type { PageContentState } from './contentModel';

/** What the content layer needs from the viewer, the same for every page. */
export interface ContentInput {
  readonly onHover: (pageIndex: number, point: PagePoint | null) => void;
  readonly onSelect: (pageIndex: number, point: PagePoint) => void;
}

interface ContentLayerProps {
  readonly pageIndex: number;
  /** CSS px per PDF point. */
  readonly scale: number;
  /** This page's parse; absent before it is asked for. */
  readonly state: PageContentState | undefined;
  readonly hovered: ContentItem | null;
  readonly selected: ContentItem | null;
  readonly input: ContentInput;
}

export function ContentLayer({ pageIndex, scale, state, hovered, selected, input }: ContentLayerProps) {
  const pointOf = (event: PointerEvent<HTMLDivElement>): PagePoint => {
    const box = event.currentTarget.getBoundingClientRect();
    return toPagePoint(event.clientX - box.left, event.clientY - box.top, scale);
  };
  return (
    <div
      className="content-layer"
      data-state={state?.status ?? 'waiting'}
      onPointerMove={(event) => input.onHover(pageIndex, pointOf(event))}
      onPointerLeave={() => input.onHover(pageIndex, null)}
      onPointerDown={(event) => {
        if (event.button !== 0 || !event.isPrimary) return;
        // The press is the mode's: no text selection starts under it.
        event.preventDefault();
        input.onSelect(pageIndex, pointOf(event));
      }}
    >
      {hovered && !sameBox(hovered, selected) && <Outline item={hovered} scale={scale} state="hover" />}
      {selected && <Outline item={selected} scale={scale} state="selected" />}
    </div>
  );
}

function Outline({ item, scale, state }: { item: ContentItem; scale: number; state: 'hover' | 'selected' }) {
  const { left, top, width, height } = item.rect;
  return (
    <div
      className={`content-outline content-outline--${item.kind} content-outline--${state}`}
      data-kind={item.kind}
      data-state={state}
      style={{ left: left * scale, top: top * scale, width: width * scale, height: height * scale }}
    />
  );
}

function sameBox(a: ContentItem, b: ContentItem | null): boolean {
  return !!b && a.kind === b.kind && a.id === b.id;
}
