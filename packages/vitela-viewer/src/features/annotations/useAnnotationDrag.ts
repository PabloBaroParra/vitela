// Dragging an annotation on one page, by its body or by a corner handle: the
// pointer gesture, what the page shows on the way, and the one change it
// asks for when it ends. Nothing goes to the engine until the pointer is let
// go, and a press that does not travel asks for nothing. Escape, a cancelled
// pointer or a lost capture drop the drag. Container logic only.
//
// The drag keeps the annotation and the page it began on: the queue drops
// it (and says so) if either is no longer as it was when the drag lands.

import { useCallback, useLayoutEffect, useMemo, useRef, useState, type PointerEvent } from 'react';
import type { Annotation, PagePoint, PageSize } from '@vitela/core';
import { toPagePoint } from '../text/pageRects';
import { dragBuild, type EditBuild } from './editBuilds';
import { previewEdit, type Corner } from './editGeometry';
import type { AnnotationInput } from './useAnnotationEditing';

/** How far the pointer must travel, in CSS px, for a press to be a drag. */
export const DRAG_SLOP_PX = 3;

interface Grab {
  readonly pointerId: number;
  readonly element: HTMLElement;
  readonly annotation: Annotation;
  readonly corner: Corner | null;
  readonly pageKey: string;
  readonly from: PagePoint;
  /** The change the drag makes if it ends now; `null` until it has travelled. */
  readonly build: EditBuild | null;
}

export interface AnnotationDrag {
  /** A press on `annotation`, or on one of its corners. */
  readonly start: (event: PointerEvent<HTMLElement>, annotation: Annotation, corner: Corner | null) => void;
  readonly move: (event: PointerEvent<HTMLElement>) => void;
  readonly finish: (event: PointerEvent<HTMLElement>) => void;
  /** The browser took the pointer away. */
  readonly drop: (event: PointerEvent<HTMLElement>) => void;
  /** Gives the drag up; `false` when there is none. */
  readonly cancel: () => boolean;
}

export function useAnnotationDrag(
  annotations: readonly Annotation[],
  page: PageSize,
  pageKey: string,
  scale: number,
  input: AnnotationInput | null,
): { readonly shown: readonly Annotation[]; readonly drag: AnnotationDrag } {
  // The gesture's own copy: moves in one frame must not read a stale state.
  const grab = useRef<Grab | null>(null);
  /** Where the drag has the annotation, shown in place of the snapshot's. */
  const [preview, setPreview] = useState<Annotation | null>(null);
  /** Where a finished drag left it, until its change has landed; the next drag starts from here. */
  const [landing, setLanding] = useState<Annotation | null>(null);

  const pointOf = useCallback(
    (event: PointerEvent<HTMLElement>) => {
      // Within the page, so a scroll mid-drag is part of the way travelled.
      const layer = event.currentTarget.closest('.annotation-edit-layer') ?? event.currentTarget;
      const box = layer.getBoundingClientRect();
      return toPagePoint(event.clientX - box.left, event.clientY - box.top, scale);
    },
    [scale],
  );

  const follow = useCallback(
    (held: Grab, event: PointerEvent<HTMLElement>): Grab => {
      const point = pointOf(event);
      const [dx, dy] = [point.x - held.from.x, point.y - held.from.y];
      if (!held.build && Math.hypot(dx, dy) * scale < DRAG_SLOP_PX) return held;
      return { ...held, build: dragBuild(held.corner, dx, dy) };
    },
    [pointOf, scale],
  );

  const start = useCallback<AnnotationDrag['start']>(
    (event, annotation, corner) => {
      if (event.button !== 0 || !event.isPrimary) return;
      // No text selection and no native drag; the focus is moved by hand, without scrolling.
      event.preventDefault();
      event.currentTarget.focus({ preventScroll: true });
      event.currentTarget.setPointerCapture?.(event.pointerId);
      grab.current = { pointerId: event.pointerId, element: event.currentTarget, annotation, corner, pageKey, from: pointOf(event), build: null };
      setPreview(null);
    },
    [pageKey, pointOf],
  );

  const move = useCallback<AnnotationDrag['move']>(
    (event) => {
      const held = grab.current;
      if (!held || held.pointerId !== event.pointerId) return;
      // The primary button went up while another is still down: no pointerup says so.
      const next = event.buttons & 1 ? follow(held, event) : null;
      grab.current = next;
      if (!next?.build) return setPreview(null);
      const edit = next.build(next.annotation, page);
      setPreview(edit ? previewEdit(next.annotation, edit) : next.annotation);
    },
    [follow, page],
  );

  const finish = useCallback<AnnotationDrag['finish']>(
    (event) => {
      const held = grab.current;
      if (!held || held.pointerId !== event.pointerId) return;
      grab.current = null;
      setPreview(null);
      if (event.button !== 0) return;
      event.currentTarget.releasePointerCapture?.(event.pointerId);
      const { annotation, build, pageKey: key } = follow(held, event);
      const edit = build?.(annotation, page);
      if (!build || !edit || !input) return;
      const left = previewEdit(annotation, edit);
      setLanding(left);
      const landed = () => setLanding((shown) => (shown === left ? null : shown));
      void input.change(annotation, key, build, annotation).then(landed, landed);
    },
    [follow, input, page],
  );

  const cancel = useCallback(() => {
    const held = grab.current;
    grab.current = null;
    if (held) setPreview(null);
    return !!held;
  }, []);
  // What was pressed left the page (an undo, an armed tool, a deselect): the
  // browser tells the lost capture to the document, where React never sees it.
  useLayoutEffect(() => {
    if (grab.current && !grab.current.element.isConnected) cancel();
  });
  const drop = useCallback<AnnotationDrag['drop']>(
    (event) => {
      if (grab.current?.pointerId === event.pointerId) cancel();
    },
    [cancel],
  );

  const shown = useMemo(() => {
    if (!preview && !landing) return annotations;
    return annotations.map((annotation) => [preview, landing].find((moved) => moved?.id === annotation.id) ?? annotation);
  }, [annotations, preview, landing]);
  const drag = useMemo(() => ({ start, move, finish, drop, cancel }), [start, move, finish, drop, cancel]);
  return { shown, drag };
}
