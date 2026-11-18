// Annotations across the worker boundary: the `AnnotationEdit` union becomes
// one of core/pdf-wasm's `EditCommand`s, and the core's flat entries
// (`AnnotationEntry`, `CommentEntry`, `FreeTextLines` — one class for every
// kind, see core/pdf-wasm/src/dto/annotation.rs) become the typed unions in
// core/types.ts.
//
// The core speaks the page's unrotated PDF space (bottom-left origin), as
// pdf-ffi does; everything the UI sees is on the page as drawn (top-left
// origin, `/Rotate` applied), like search and selection. The transform is the
// core's own (`placeRects` / `pointToPdf`), so no rotation math lives here.
//
// The decoders never trust a field the kind does not have: wasm-bindgen types
// an absent `Option` as present in its `.d.ts` (T-227), and an entry of a kind
// this package does not know is skipped rather than guessed at.

import type {
  Annotation,
  AnnotationEdit,
  Comment,
  FreeTextLayout,
  PagePoint,
  PageRect,
  PageSize,
  Rgb,
} from '../core/types.js';
import { ANNOTATION_NOT_FOUND, EngineError } from '../core/errors.js';
import type { CoreGeometry, Owned } from './pageText.js';

/** core/pdf-wasm's annotation `EditCommand` constructors. Geometry in PDF space; colour `0xRRGGBB`. */
export interface CoreAnnotationEdits {
  addHighlight(page: number, x: number, y: number, width: number, height: number, rgb: number): Owned;
  addUnderline(page: number, x: number, y: number, width: number, height: number, rgb: number): Owned;
  addStrikeout(page: number, x: number, y: number, width: number, height: number, rgb: number): Owned;
  addShape(page: number, x: number, y: number, width: number, height: number, rgb: number): Owned;
  addInk(page: number, points: Float64Array, rgb: number): Owned;
  addNote(page: number, x: number, y: number, width: number, height: number, contents: string): Owned;
  addFreeText(page: number, x: number, y: number, width: number, height: number, contents: string): Owned;
  addStamp(page: number, image: Uint8Array, x: number, y: number, width: number, height: number): Owned;
  moveAnnotation(id: number, dx: number, dy: number): Owned;
  resizeAnnotation(id: number, x: number, y: number, width: number, height: number): Owned;
  recolorAnnotation(id: number, rgb: number): Owned;
  retextAnnotation(id: number, contents: string): Owned;
  removeAnnotation(id: number): Owned;
}

/** core/pdf-wasm's `FreeTextLines`. */
export interface CoreFreeTextLines extends Owned {
  readonly fontSizePt: number;
  readonly texts: string[];
  readonly xs: Float64Array;
  readonly baselines: Float64Array;
  readonly overflow: boolean;
}

/** core/pdf-wasm's `AnnotationEntry`: every field for every kind, empty when the kind has none. */
export interface CoreAnnotation extends Owned {
  readonly id: number;
  readonly page: number;
  readonly kind: string;
  /** `x, y, width, height`; empty for ink. */
  readonly rect: Float64Array;
  /** `x0, y0, x1, y1, ...`; only for ink. */
  readonly points: Float64Array;
  readonly color: number | undefined;
  readonly contents: string;
  readonly layout: CoreFreeTextLines | undefined;
}

/** core/pdf-wasm's `CommentEntry`. */
export interface CoreComment extends Owned {
  readonly page: number;
  readonly rect: Float64Array;
  readonly contents: string;
  /** `undefined` (typed `string` by wasm-bindgen) when there is none. */
  readonly author: string | null | undefined;
  readonly date: string | null | undefined;
  readonly annotationId: number | undefined;
}

/** One page as drawn, and the core's transform between it and PDF space. */
export class PageFrame {
  constructor(
    private readonly page: PageSize,
    private readonly geometry: CoreGeometry,
  ) {}

  /** A drawn point in PDF space. */
  pointToPdf(point: PagePoint): [number, number] {
    const [x, y] = this.geometry.pointToPdf(point.x, point.y, this.page.widthPt, this.page.heightPt, this.page.rotationDegrees);
    return [x!, y!];
  }

  /** A drawn box in PDF space, as `[x, y, width, height]`: the box between its two corners. */
  rectToPdf(rect: PageRect): [number, number, number, number] {
    const [ax, ay] = this.pointToPdf({ x: rect.left, y: rect.top });
    const [bx, by] = this.pointToPdf({ x: rect.left + rect.width, y: rect.top + rect.height });
    return [Math.min(ax, bx), Math.min(ay, by), Math.abs(bx - ax), Math.abs(by - ay)];
  }

  /** A drawn displacement in PDF space. The transform is affine, so its linear part moves a delta. */
  deltaToPdf(dx: number, dy: number): [number, number] {
    const [ox, oy] = this.pointToPdf({ x: 0, y: 0 });
    const [x, y] = this.pointToPdf({ x: dx, y: dy });
    return [x - ox, y - oy];
  }

  /** A PDF-space `[x, y, width, height]` on the page as drawn. */
  rectFromPdf(rect: ArrayLike<number>): PageRect {
    const [left, top, width, height] = this.place(Float32Array.from(rect));
    return { left: left!, top: top!, width: width!, height: height! };
  }

  /** PDF-space `x0, y0, x1, y1, ...` on the page as drawn (each point placed as an empty box). */
  pointsFromPdf(flat: ArrayLike<number>): PagePoint[] {
    const boxes = new Float32Array((flat.length >> 1) * 4);
    for (let i = 0; i + 1 < flat.length; i += 2) {
      boxes[i * 2] = flat[i]!;
      boxes[i * 2 + 1] = flat[i + 1]!;
    }
    const placed = this.place(boxes);
    const points: PagePoint[] = [];
    for (let i = 0; i + 3 < placed.length; i += 4) points.push({ x: placed[i]!, y: placed[i + 1]! });
    return points;
  }

  private place(rects: Float32Array): Float32Array {
    return this.geometry.placeRects(rects, this.page.widthPt, this.page.heightPt, this.page.rotationDegrees);
  }
}

/** `color` as `0xRRGGBB`, each channel rounded and clamped to 0-255. */
export function packRgb(color: Rgb): number {
  const channel = (value: number) => Math.min(255, Math.max(0, Math.round(value)));
  return (channel(color.r) << 16) | (channel(color.g) << 8) | channel(color.b);
}

export function unpackRgb(rgb: number): Rgb {
  return { r: (rgb >> 16) & 0xff, g: (rgb >> 8) & 0xff, b: rgb & 0xff };
}

/**
 * The core command for `edit`. `frameOf` gives the frame of a page by
 * position, `pageOf` the position of an annotation's page in the snapshot,
 * and `editable` whether the document lets annotations be edited at all.
 *
 * A page past the end, or an id given without geometry, crosses
 * untransformed and the core refuses it, so the order of refusals stays the
 * core's (the annotate permission first, then the page or the id).
 *
 * A move or resize is the exception: its geometry is on a drawn page, and an
 * annotation the snapshot does not hold (an unknown id, or one whose page was
 * removed — the core keeps that one for an undo and would accept the change)
 * has no page to convert it on. Sent as is, the core would record a step with
 * drawn coordinates taken for PDF ones, so it is refused here with the core's
 * `AnnotationNotFound` — after the permission, which a document that refuses
 * annotation edits is still left to the core to name.
 */
export function annotationCommand(
  edits: CoreAnnotationEdits,
  edit: AnnotationEdit,
  frameOf: (page: number) => PageFrame | undefined,
  pageOf: (id: number) => number | undefined,
  editable: () => boolean,
): Owned {
  const box = (page: number, rect: PageRect) =>
    frameOf(page)?.rectToPdf(rect) ?? ([rect.left, rect.top, rect.width, rect.height] as const);
  /** The frame to convert annotation `id`'s geometry on, `undefined` for the core to refuse first. */
  const frameOfAnnotation = (id: number) => {
    const page = pageOf(id);
    const frame = page === undefined ? undefined : frameOf(page);
    if (!frame && editable()) throw new EngineError(ANNOTATION_NOT_FOUND, `no annotation with id ${id}`);
    return frame;
  };
  switch (edit.kind) {
    case 'addMarkup': {
      const [x, y, width, height] = box(edit.page, edit.rect);
      const rgb = packRgb(edit.color);
      if (edit.markup === 'highlight') return edits.addHighlight(edit.page, x, y, width, height, rgb);
      if (edit.markup === 'underline') return edits.addUnderline(edit.page, x, y, width, height, rgb);
      return edits.addStrikeout(edit.page, x, y, width, height, rgb);
    }
    case 'addShape': {
      const [x, y, width, height] = box(edit.page, edit.rect);
      return edits.addShape(edit.page, x, y, width, height, packRgb(edit.color));
    }
    case 'addInk': {
      const frame = frameOf(edit.page);
      const flat = new Float64Array(edit.points.length * 2);
      edit.points.forEach((point, i) => flat.set(frame?.pointToPdf(point) ?? [point.x, point.y], i * 2));
      return edits.addInk(edit.page, flat, packRgb(edit.color));
    }
    case 'addNote': {
      const [x, y, width, height] = box(edit.page, edit.rect);
      return edits.addNote(edit.page, x, y, width, height, edit.contents);
    }
    case 'addFreeText': {
      const [x, y, width, height] = box(edit.page, edit.rect);
      return edits.addFreeText(edit.page, x, y, width, height, edit.contents);
    }
    case 'addStamp': {
      const [x, y, width, height] = box(edit.page, edit.rect);
      return edits.addStamp(edit.page, edit.image, x, y, width, height);
    }
    case 'moveAnnotation': {
      const [dx, dy] = frameOfAnnotation(edit.id)?.deltaToPdf(edit.dx, edit.dy) ?? [edit.dx, edit.dy];
      return edits.moveAnnotation(edit.id, dx, dy);
    }
    case 'resizeAnnotation': {
      const { left, top, width, height } = edit.rect;
      const [x, y, w, h] = frameOfAnnotation(edit.id)?.rectToPdf(edit.rect) ?? [left, top, width, height];
      return edits.resizeAnnotation(edit.id, x, y, w, h);
    }
    case 'recolorAnnotation':
      return edits.recolorAnnotation(edit.id, packRgb(edit.color));
    case 'retextAnnotation':
      return edits.retextAnnotation(edit.id, edit.contents);
    case 'removeAnnotation':
      return edits.removeAnnotation(edit.id);
  }
}

/** Copies the core's layout into a plain object (the caller frees the core's). */
export function freeTextLayout(lines: CoreFreeTextLines): FreeTextLayout {
  const { texts, xs, baselines } = lines;
  return {
    fontSizePt: lines.fontSizePt,
    lines: texts.map((text, i) => ({ text, xPt: xs[i] ?? 0, baselineFromTopPt: baselines[i] ?? 0 })),
    overflow: lines.overflow,
  };
}

/**
 * Copies the core's entries into plain annotations on the page as drawn and
 * frees them. An entry of a kind this package does not know, or missing what
 * its kind needs, is left out.
 */
export function annotations(entries: CoreAnnotation[], frameOf: (page: number) => PageFrame | undefined): Annotation[] {
  const decoded: Annotation[] = [];
  for (const entry of entries) {
    try {
      const frame = frameOf(entry.page);
      const annotation = frame && decodeAnnotation(entry, frame);
      if (annotation) decoded.push(annotation);
    } finally {
      entry.free();
    }
  }
  return decoded;
}

function decodeAnnotation(entry: CoreAnnotation, frame: PageFrame): Annotation | null {
  const base = { id: entry.id, pageIndex: entry.page };
  const kind = entry.kind;
  if (kind === 'ink') {
    if (entry.color === undefined) return null;
    return { ...base, kind, points: frame.pointsFromPdf(entry.points), color: unpackRgb(entry.color) };
  }
  if (entry.rect.length !== 4) return null;
  const rect = frame.rectFromPdf(entry.rect);
  switch (kind) {
    case 'highlight':
    case 'underline':
    case 'strikeout':
    case 'shape':
      return entry.color === undefined ? null : { ...base, kind, rect, color: unpackRgb(entry.color) };
    case 'note':
      return { ...base, kind, rect, contents: entry.contents };
    case 'stamp':
      return { ...base, kind, rect };
    case 'freeText': {
      const layout = entry.layout;
      if (!layout) return null;
      try {
        return { ...base, kind, rect, contents: entry.contents, layout: freeTextLayout(layout) };
      } finally {
        layout.free();
      }
    }
    default:
      return null;
  }
}

/** Copies the core's comments into plain ones on the page as drawn and frees them. */
export function comments(entries: CoreComment[], frameOf: (page: number) => PageFrame | undefined): Comment[] {
  const decoded: Comment[] = [];
  for (const entry of entries) {
    try {
      const frame = frameOf(entry.page);
      if (!frame || entry.rect.length !== 4) continue;
      decoded.push({
        pageIndex: entry.page,
        rect: frame.rectFromPdf(entry.rect),
        contents: entry.contents,
        author: entry.author ?? null,
        date: entry.date ?? null,
        annotationId: entry.annotationId ?? null,
      });
    } finally {
      entry.free();
    }
  }
  return decoded;
}
