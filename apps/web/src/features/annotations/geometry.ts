// Geometry of the annotation layer: what the engine's snapshot (boxes and
// points on the page as drawn, PDF points from its top-left corner, `/Rotate`
// applied) becomes on screen, and a text selection turned into markup edits.
// Pure: the layer draws in an SVG whose units are those same points, so no
// zoom math lives here.

import type { AnnotationEdit, MarkupKind, PageRect, PageRects, Rgb } from '@vitela/core';

/** How thick an underline or strikeout is drawn, in points. */
export const RULE_PT = 1.5;

/** A straight line on the page as drawn, in points. */
export interface Segment {
  readonly x1: number;
  readonly y1: number;
  readonly x2: number;
  readonly y2: number;
}

/** A page's turn as 0, 90, 180 or 270. */
function quarterTurn(rotationDegrees: number): 0 | 90 | 180 | 270 {
  const turn = (((Math.round(rotationDegrees / 90) * 90) % 360) + 360) % 360;
  return turn as 0 | 90 | 180 | 270;
}

/**
 * Where the rule of an underline or strikeout goes in its drawn box. The box
 * is the annotation's PDF rect turned with the page, and the saved file draws
 * the underline along that rect's PDF bottom edge, so on a turned page the
 * rule follows that edge (a quarter turn clockwise takes it to the drawn
 * left), kept inside the box by half its thickness. A strikeout runs through
 * the middle, along the text.
 */
export function markupRule(kind: 'underline' | 'strikeout', rect: PageRect, rotationDegrees: number): Segment {
  const { left, top, width, height } = rect;
  const right = left + width;
  const bottom = top + height;
  const turn = quarterTurn(rotationDegrees);
  const along = turn === 0 || turn === 180; // text runs left to right (or upside down)
  if (kind === 'strikeout') {
    if (along) return { x1: left, y1: top + height / 2, x2: right, y2: top + height / 2 };
    return { x1: left + width / 2, y1: top, x2: left + width / 2, y2: bottom };
  }
  const half = RULE_PT / 2;
  switch (turn) {
    case 0:
      return { x1: left, y1: bottom - half, x2: right, y2: bottom - half };
    case 90:
      return { x1: left + half, y1: top, x2: left + half, y2: bottom };
    case 180:
      return { x1: left, y1: top + half, x2: right, y2: top + half };
    case 270:
      return { x1: right - half, y1: top, x2: right - half, y2: bottom };
  }
}

/** A free-text box's own frame (before the page's turn) and the SVG transform that places it. */
export interface TextFrame {
  readonly width: number;
  readonly height: number;
  readonly transform: string;
}

/**
 * The core lays a free-text box's lines out in the box's own frame, which
 * turns with the page (as any viewer draws the saved box). The frame is the
 * drawn box with its sides swapped on a quarter turn, turned about its centre.
 */
export function freeTextFrame(rect: PageRect, rotationDegrees: number): TextFrame {
  const turn = quarterTurn(rotationDegrees);
  const sideways = turn === 90 || turn === 270;
  const width = sideways ? rect.height : rect.width;
  const height = sideways ? rect.width : rect.height;
  const cx = rect.left + rect.width / 2;
  const cy = rect.top + rect.height / 2;
  return { width, height, transform: `translate(${cx} ${cy}) rotate(${turn}) translate(${-width / 2} ${-height / 2})` };
}

/** Floats per rect in a `PageRects` buffer: left, top, width, height. */
const STRIDE = 4;

/**
 * One `addMarkup` per line of a selection: the core's markup has one box (no
 * `/QuadPoints`), so a three-line selection is three annotations and three
 * undo steps, as in the native shells. Empty boxes are skipped.
 */
export function markupEdits(page: number, rects: PageRects, markup: MarkupKind, color: Rgb): AnnotationEdit[] {
  const edits: AnnotationEdit[] = [];
  for (let i = 0; i + STRIDE <= rects.length; i += STRIDE) {
    const [left, top, width, height] = [rects[i]!, rects[i + 1]!, rects[i + 2]!, rects[i + 3]!];
    if (width <= 0 || height <= 0) continue;
    edits.push({ kind: 'addMarkup', page, markup, rect: { left, top, width, height }, color });
  }
  return edits;
}

export function colorCss({ r, g, b }: Rgb): string {
  return `rgb(${r} ${g} ${b})`;
}
