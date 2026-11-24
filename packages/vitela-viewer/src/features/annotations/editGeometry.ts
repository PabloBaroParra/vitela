// Geometry of editing an annotation already on the page: the box it takes
// up, the area a press grabs it by, its corner handles, and what a drag, a
// key or a typed size makes of its box. Plus which edits each kind takes —
// the core's rules (`pdf_annotate::ops`), so the UI never offers one the
// core would refuse. Pure; points on the page as drawn, like the snapshot.

import type { Annotation, AnnotationEdit, PagePoint, PageRect, PageSize } from '@vitela/core';
import { drawnSize, FREE_TEXT_MIN_SIZE, type Size } from './drawing';

/** How far a thin mark (a rule, an ink stroke) can be grabbed from, in points. */
export const HIT_SLOP_PT = 3;
/** The smallest side a resize leaves (the native shells' `MIN_TRACED_PT`). */
export const MIN_RESIZE_PT = 4;

/**
 * Kept over the core's text-box minimum: the snapshot's boxes are 32-bit, so
 * a side left exactly at the minimum can come back a hair under it.
 */
const FLOOR_MARGIN_PT = 0.01;

/** A corner of a box: its handle resizes it, the opposite corner stays put. */
export type Corner = 'nw' | 'ne' | 'sw' | 'se';
export const CORNERS: readonly Corner[] = ['nw', 'ne', 'sw', 'se'];

/** The box an annotation takes up: its rect, or the extent of an ink stroke. */
export function annotationBounds(annotation: Annotation): PageRect {
  if (annotation.kind !== 'ink') return annotation.rect;
  const xs = annotation.points.map((point) => point.x);
  const ys = annotation.points.map((point) => point.y);
  const left = Math.min(...xs);
  const top = Math.min(...ys);
  return { left, top, width: Math.max(...xs) - left, height: Math.max(...ys) - top };
}

/** Where a press grabs the annotation: its box, grown for marks drawn as a thin line. */
export function hitArea(annotation: Annotation): PageRect {
  const bounds = annotationBounds(annotation);
  const thin = annotation.kind === 'underline' || annotation.kind === 'strikeout' || annotation.kind === 'ink';
  if (!thin) return bounds;
  return {
    left: bounds.left - HIT_SLOP_PT,
    top: bounds.top - HIT_SLOP_PT,
    width: bounds.width + 2 * HIT_SLOP_PT,
    height: bounds.height + 2 * HIT_SLOP_PT,
  };
}

/** Ink has no box of its own to replace (`resize_annotation`). */
export function canResize(annotation: Annotation): boolean {
  return annotation.kind !== 'ink';
}

/** Notes, stamps and free text carry no colour in the core (`restyle_annotation`). */
export function canRecolor(annotation: Annotation): boolean {
  return annotation.kind !== 'note' && annotation.kind !== 'stamp' && annotation.kind !== 'freeText';
}

/** Only a text box's text can be retyped (`set_annotation_contents`). */
export function canRetext(annotation: Annotation): boolean {
  return annotation.kind === 'freeText';
}

/**
 * A stamp's box has its image's proportions (the core sized it), and the
 * image fills the box: handles and keys keep them so it is never squashed.
 */
export function keepsProportions(annotation: Annotation): boolean {
  return annotation.kind === 'stamp';
}

/**
 * The smallest box a resize may leave: just over a text box's core minimum
 * (one line of its frame, sideways on a quarter-turned page), a few points
 * otherwise — or the side it already has, so a thinner box never grows on
 * the side left alone.
 */
export function resizeFloor(annotation: Annotation, rotationDegrees: number): Size {
  if (annotation.kind === 'freeText') {
    const { width, height } = FREE_TEXT_MIN_SIZE;
    return drawnSize({ width: width + FLOOR_MARGIN_PT, height: height + FLOOR_MARGIN_PT }, rotationDegrees);
  }
  const { width, height } = annotationBounds(annotation);
  return { width: Math.min(MIN_RESIZE_PT, width), height: Math.min(MIN_RESIZE_PT, height) };
}

/** A move of `bounds` by (`dx`, `dy`), cut short so the box stays on the page. */
export function clampMove(bounds: PageRect, dx: number, dy: number, page: PageSize): { dx: number; dy: number } {
  const left = clamp(bounds.left + dx, 0, page.widthPt - bounds.width);
  const top = clamp(bounds.top + dy, 0, page.heightPt - bounds.height);
  return { dx: left - bounds.left, dy: top - bounds.top };
}

export function cornerPoint(rect: PageRect, corner: Corner): PagePoint {
  const x = corner === 'nw' || corner === 'sw' ? rect.left : rect.left + rect.width;
  const y = corner === 'nw' || corner === 'ne' ? rect.top : rect.top + rect.height;
  return { x, y };
}

/** Which way `corner` points from the middle of its box (the other way is west or north). */
export function sides(corner: Corner): { readonly east: boolean; readonly south: boolean } {
  return { east: corner === 'ne' || corner === 'se', south: corner === 'sw' || corner === 'se' };
}

/** The page left beyond `anchor` on the side `corner` grows to. */
function roomAt(anchor: PagePoint, corner: Corner, page: PageSize): Size {
  const { east, south } = sides(corner);
  return { width: east ? page.widthPt - anchor.x : anchor.x, height: south ? page.heightPt - anchor.y : anchor.y };
}

const OPPOSITE: Record<Corner, Corner> = { nw: 'se', ne: 'sw', sw: 'ne', se: 'nw' };

/**
 * The box `rect` becomes with its `corner` dragged to `point`: the opposite
 * corner stays, the dragged one stays on its own side of it (a box never
 * turns inside out), each side at least `floor`, and the box on the page.
 * With `keep`, both sides scale by the larger pull, so the proportions hold.
 */
export function resizedRect(rect: PageRect, corner: Corner, point: PagePoint, floor: Size, page: PageSize, keep: boolean): PageRect {
  const anchor = cornerPoint(rect, OPPOSITE[corner]);
  const { east, south } = sides(corner);
  const pulledWidth = east ? point.x - anchor.x : anchor.x - point.x;
  const pulledHeight = south ? point.y - anchor.y : anchor.y - point.y;
  const room = roomAt(anchor, corner, page);
  const { width, height } = keep
    ? scaled(rect, Math.max(pulledWidth / rect.width, pulledHeight / rect.height), floor, room)
    : { width: clamp(pulledWidth, floor.width, room.width), height: clamp(pulledHeight, floor.height, room.height) };
  return {
    left: east ? anchor.x : anchor.x - width,
    top: south ? anchor.y : anchor.y - height,
    width,
    height,
  };
}

/**
 * The box `rect` becomes grown by `dw` × `dh` points (negative shrinks) at
 * its `corner`, the opposite corner kept, as a key does it. With `keep`, the
 * side the key changed sets the scale of both.
 */
export function resizedBy(
  rect: PageRect,
  corner: Corner,
  dw: number,
  dh: number,
  floor: Size,
  page: PageSize,
  keep: boolean,
): PageRect {
  const { east, south } = sides(corner);
  const anchor = cornerPoint(rect, OPPOSITE[corner]);
  const room = roomAt(anchor, corner, page);
  const { width, height } = keep
    ? scaled(rect, dw !== 0 ? (rect.width + dw) / rect.width : (rect.height + dh) / rect.height, floor, room)
    : { width: clamp(rect.width + dw, floor.width, room.width), height: clamp(rect.height + dh, floor.height, room.height) };
  return { left: east ? anchor.x : anchor.x - width, top: south ? anchor.y : anchor.y - height, width, height };
}

/** `rect`'s size times `factor`, the factor held between the floor and the room left on the page. */
function scaled(rect: PageRect, factor: number, floor: Size, room: Size): Size {
  const least = Math.max(floor.width / rect.width, floor.height / rect.height);
  const most = Math.min(room.width / rect.width, room.height / rect.height);
  const held = clamp(factor, least, most);
  return { width: rect.width * held, height: rect.height * held };
}

/** `annotation` as `edit` will leave it, for a move or resize still being dragged. */
export function previewEdit(annotation: Annotation, edit: AnnotationEdit): Annotation {
  if (edit.kind === 'resizeAnnotation' && annotation.kind !== 'ink') return { ...annotation, rect: edit.rect };
  if (edit.kind !== 'moveAnnotation') return annotation;
  const { dx, dy } = edit;
  if (annotation.kind === 'ink') return { ...annotation, points: annotation.points.map(({ x, y }) => ({ x: x + dx, y: y + dy })) };
  const { left, top, width, height } = annotation.rect;
  return { ...annotation, rect: { left: left + dx, top: top + dy, width, height } };
}

/** Under this, in points, two places are the same: the snapshot's 32-bit rounding, not a move. */
const SAME_PLACE_PT = 0.01;
const near = (a: number, b: number) => Math.abs(a - b) < SAME_PLACE_PT;

/** Whether `a` and `b` (the same annotation at two moments) sit in the same place, at the same size. */
export function sameGeometry(a: Annotation, b: Annotation): boolean {
  if (a.kind === 'ink' || b.kind === 'ink') {
    if (a.kind !== 'ink' || b.kind !== 'ink' || a.points.length !== b.points.length) return false;
    return a.points.every((point, index) => near(point.x, b.points[index]!.x) && near(point.y, b.points[index]!.y));
  }
  const [p, q] = [a.rect, b.rect];
  return near(p.left, q.left) && near(p.top, q.top) && near(p.width, q.width) && near(p.height, q.height);
}

const KIND_NAMES: Record<Annotation['kind'], string> = {
  highlight: 'Highlight',
  underline: 'Underline',
  strikeout: 'Strikeout',
  shape: 'Shape',
  ink: 'Ink',
  note: 'Note',
  freeText: 'Text box',
  stamp: 'Image stamp',
};

/** What an annotation is called to the user. */
export function kindName(annotation: Annotation): string {
  return KIND_NAMES[annotation.kind];
}

export function annotationLabel(annotation: Annotation): string {
  return `${kindName(annotation)} on page ${annotation.pageIndex + 1}`;
}

/** `value` held between `min` and `max`; `min` wins when they cross. */
function clamp(value: number, min: number, max: number): number {
  return Math.max(Math.min(value, max), min);
}
