import { describe, expect, it } from 'vitest';
import type { Annotation, PageSize } from '@vitela/core';
import { dragBuild, moveBy, moveTo, recolor, remove, resizeBy, resizeTo, retext } from './editBuilds';

const LETTER: PageSize = { widthPt: 612, heightPt: 792, rotationDegrees: 0 };
const RED = { r: 220, g: 40, b: 40 };
const box = { left: 100, top: 200, width: 80, height: 40 };
const shape: Annotation = { id: 3, pageIndex: 0, kind: 'shape', rect: box, color: RED };
const stamp: Annotation = { id: 6, pageIndex: 0, kind: 'stamp', rect: box };
const note: Annotation = { id: 5, pageIndex: 0, kind: 'note', rect: box, contents: 'n' };
const ink: Annotation = {
  id: 4,
  pageIndex: 0,
  kind: 'ink',
  points: [
    { x: 10, y: 20 },
    { x: 30, y: 40 },
  ],
  color: RED,
};
const text: Annotation = {
  id: 7,
  pageIndex: 0,
  kind: 'freeText',
  rect: box,
  contents: 't',
  layout: { fontSizePt: 12, lines: [], overflow: false },
};

describe('moving', () => {
  it('moves by a step, kept on the page', () => {
    expect(moveBy(5, -3)(shape, LETTER)).toEqual({ kind: 'moveAnnotation', id: 3, dx: 5, dy: -3 });
    expect(moveBy(-500, 0)(ink, LETTER)).toEqual({ kind: 'moveAnnotation', id: 4, dx: -10, dy: 0 });
  });

  it('asks for nothing when nothing moves (or the box is already at the edge)', () => {
    expect(moveBy(0, 0)(shape, LETTER)).toBeNull();
    expect(moveBy(-1, 0)({ ...shape, rect: { ...box, left: 0 } } as Annotation, LETTER)).toBeNull();
  });

  it('moves to a place from wherever the annotation is by then', () => {
    expect(moveTo(10, 20)(shape, LETTER)).toEqual({ kind: 'moveAnnotation', id: 3, dx: -90, dy: -180 });
    expect(moveTo(100, 200)(shape, LETTER)).toBeNull();
  });
});

describe('resizing', () => {
  it('resizes by a step, a stamp in proportion', () => {
    expect(resizeBy('se', 10, 0)(shape, LETTER)).toEqual({ kind: 'resizeAnnotation', id: 3, rect: { ...box, width: 90 } });
    expect(resizeBy('se', 40, 0)(stamp, LETTER)).toEqual({
      kind: 'resizeAnnotation',
      id: 6,
      rect: { ...box, width: 120, height: 60 },
    });
  });

  it('resizes to a size with the top-left corner kept', () => {
    expect(resizeTo(50, 60)(shape, LETTER)).toEqual({
      kind: 'resizeAnnotation',
      id: 3,
      rect: { ...box, width: 50, height: 60 },
    });
    expect(resizeTo(80, 40)(shape, LETTER)).toBeNull();
  });

  it('never resizes ink, which has no box', () => {
    expect(resizeBy('se', 10, 0)(ink, LETTER)).toBeNull();
    expect(resizeTo(50, 60)(ink, LETTER)).toBeNull();
  });
});

describe('dragBuild', () => {
  it('moves by the body and resizes by a corner', () => {
    expect(dragBuild(null, 7, 8)(shape, LETTER)).toEqual({ kind: 'moveAnnotation', id: 3, dx: 7, dy: 8 });
    expect(dragBuild('nw', -10, -20)(shape, LETTER)).toEqual({
      kind: 'resizeAnnotation',
      id: 3,
      rect: { left: 90, top: 180, width: 90, height: 60 },
    });
    expect(dragBuild('se', 40, 5)(stamp, LETTER)).toMatchObject({ rect: { width: 120, height: 60 } });
  });

  it('makes nothing of a drag that ends where it began', () => {
    expect(dragBuild(null, 0, 0)(shape, LETTER)).toBeNull();
    expect(dragBuild('se', 0, 0)(shape, LETTER)).toBeNull();
  });
});

describe('recolor, retext and remove', () => {
  it('recolours only what has a colour, and only to another one', () => {
    const blue = { r: 0, g: 0, b: 255 };
    expect(recolor(blue)(shape, LETTER)).toEqual({ kind: 'recolorAnnotation', id: 3, color: blue });
    expect(recolor(RED)(shape, LETTER)).toBeNull();
    expect(recolor(blue)(stamp, LETTER)).toBeNull();
  });

  it('retypes only a text box, and only to another text', () => {
    expect(retext('new')(text, LETTER)).toEqual({ kind: 'retextAnnotation', id: 7, contents: 'new' });
    expect(retext('t')(text, LETTER)).toBeNull();
    expect(retext('new')(note, LETTER)).toBeNull();
  });

  it('removes any kind', () => {
    expect(remove(note, LETTER)).toEqual({ kind: 'removeAnnotation', id: 5 });
  });
});
