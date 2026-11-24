// What each way of editing an annotation asks of the core, as a function of
// the annotation as it is when the edit's turn in the queue comes
// (`useAnnotationEdits.change`): a key pressed twice builds its second move
// from where the first one left the annotation. `null` asks for nothing, so
// nothing is recorded: a drag back to where it began, a nudge against the
// page's edge, the colour or text it already has, or an edit the kind does
// not take (the core refuses those). Pure.

import type { Annotation, AnnotationEdit, PageRect, PageSize, Rgb } from '@vitela/core';
import {
  annotationBounds,
  canRecolor,
  canResize,
  canRetext,
  clampMove,
  cornerPoint,
  keepsProportions,
  resizedBy,
  resizedRect,
  resizeFloor,
  type Corner,
} from './editGeometry';

/** Builds an edit of `current` on `page` (its page as drawn); `null` for nothing to do. */
export type EditBuild = (current: Annotation, page: PageSize) => AnnotationEdit | null;

/** Less than this, in points, is rounding and not a change. */
const NEGLIGIBLE_PT = 0.001;
const negligible = (...lengths: number[]) => lengths.every((length) => Math.abs(length) < NEGLIGIBLE_PT);

function moveEdit(id: number, dx: number, dy: number): AnnotationEdit | null {
  return negligible(dx, dy) ? null : { kind: 'moveAnnotation', id, dx, dy };
}

export function moveBy(dx: number, dy: number): EditBuild {
  return (current, page) => {
    const move = clampMove(annotationBounds(current), dx, dy, page);
    return moveEdit(current.id, move.dx, move.dy);
  };
}

/** Puts the top-left corner at (`left`, `top`). */
export function moveTo(left: number, top: number): EditBuild {
  return (current) => {
    const bounds = annotationBounds(current);
    return moveEdit(current.id, left - bounds.left, top - bounds.top);
  };
}

function resizeEdit(current: Annotation, rectOf: (rect: PageRect) => PageRect) {
  if (!canResize(current)) return null;
  const before = annotationBounds(current);
  const rect = rectOf(before);
  const same = negligible(
    rect.left - before.left,
    rect.top - before.top,
    rect.width - before.width,
    rect.height - before.height,
  );
  return same ? null : ({ kind: 'resizeAnnotation', id: current.id, rect } as const);
}

/** Grows the box by `dw` × `dh` points at `corner` (negative shrinks), the opposite corner kept. */
export function resizeBy(corner: Corner, dw: number, dh: number): EditBuild {
  return (current, page) =>
    resizeEdit(current, (rect) =>
      resizedBy(rect, corner, dw, dh, resizeFloor(current, page.rotationDegrees), page, keepsProportions(current)),
    );
}

/** Gives the box a size, its top-left corner kept. */
export function resizeTo(width: number, height: number): EditBuild {
  return (current) => resizeEdit(current, ({ left, top }) => ({ left, top, width, height }));
}

/** A drag by (`dx`, `dy`): of the annotation itself, or of one of its corners. */
export function dragBuild(corner: Corner | null, dx: number, dy: number): EditBuild {
  if (!corner) return moveBy(dx, dy);
  return (current, page) =>
    resizeEdit(current, (rect) => {
      const from = cornerPoint(rect, corner);
      const floor = resizeFloor(current, page.rotationDegrees);
      return resizedRect(rect, corner, { x: from.x + dx, y: from.y + dy }, floor, page, keepsProportions(current));
    });
}

export function recolor(color: Rgb): EditBuild {
  return (current) => {
    if (!canRecolor(current) || !('color' in current)) return null;
    const { r, g, b } = current.color;
    if (r === color.r && g === color.g && b === color.b) return null;
    return { kind: 'recolorAnnotation', id: current.id, color };
  };
}

export function retext(contents: string): EditBuild {
  return (current) => {
    if (!canRetext(current) || !('contents' in current) || current.contents === contents) return null;
    return { kind: 'retextAnnotation', id: current.id, contents };
  };
}

export const remove: EditBuild = (current) => ({ kind: 'removeAnnotation', id: current.id });
