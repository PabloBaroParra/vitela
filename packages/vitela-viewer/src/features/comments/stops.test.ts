import { describe, expect, it } from 'vitest';
import type { Annotation, Comment } from '@vitela/core';
import { commentEntries, stepStop, stopLabel, stopsOf } from './stops';

const rect = { left: 10, top: 20, width: 30, height: 40 };
const color = { r: 1, g: 2, b: 3 };
const shape = (id: number, pageIndex: number): Annotation => ({ id, pageIndex, kind: 'shape', rect, color });
const note = (id: number, pageIndex: number): Annotation => ({ id, pageIndex, kind: 'note', rect, contents: 'mine' });
const comment = (pageIndex: number, annotationId: number | null = null): Comment => ({
  pageIndex,
  rect,
  contents: 'text',
  author: null,
  date: null,
  annotationId,
});
const keyOf = (index: number) => ['a', 'b', 'c'][index]!;
const keys = (annotations: Annotation[], comments: Comment[]) =>
  stopsOf(annotations, commentEntries(comments, keyOf)).map(({ key }) => key);

describe('commentEntries', () => {
  it('names a session note by its id and a file comment by its page and place there', () => {
    const entries = commentEntries([comment(0), comment(0), comment(0, 7), comment(2)], keyOf);
    expect(entries.map(({ key }) => key)).toEqual(['file:a:0', 'file:a:1', 'session:7', 'file:c:0']);
  });

  it('keeps the name of a file comment whose page moved', () => {
    const before = commentEntries([comment(0)], keyOf)[0]!.key;
    expect(commentEntries([comment(1)], (index) => ['b', 'a'][index]!)[0]!.key).toBe(before);
  });
});

describe('stopsOf', () => {
  it('goes page by page, the file comments first, then the session in paint order', () => {
    expect(keys([shape(9, 1), shape(4, 0), shape(2, 0)], [comment(0), comment(1)])).toEqual([
      'file:a:0',
      'session:4',
      'session:2',
      'file:b:0',
      'session:9',
    ]);
  });

  it('stops once at a session note, which the comments list too', () => {
    expect(keys([note(7, 0)], [comment(0, 7)])).toEqual(['session:7']);
  });

  it('has the session annotations while the comments are not known', () => {
    expect(keys([shape(1, 0)], [])).toEqual(['session:1']);
  });

  it('names each stop, and where an ink stroke is', () => {
    const ink: Annotation = {
      id: 3,
      pageIndex: 1,
      kind: 'ink',
      color,
      points: [
        { x: 5, y: 9 },
        { x: 8, y: 2 },
      ],
    };
    const stops = stopsOf([ink], commentEntries([comment(0)], keyOf));
    expect(stops.map(stopLabel)).toEqual(['Comment on page 1', 'Ink on page 2']);
    expect(stops[1]!.rect).toEqual({ left: 5, top: 2, width: 3, height: 7 });
    expect(stops.map(({ annotationId }) => annotationId)).toEqual([null, 3]);
  });
});

describe('stepStop', () => {
  const stops = stopsOf([shape(1, 0), shape(2, 0), shape(3, 1)], []);
  const step = (key: string | null, forward: boolean) => stepStop(stops, key, forward)?.key;

  it('enters at the first going forward and at the last going back', () => {
    expect([step(null, true), step(null, false)]).toEqual(['session:1', 'session:3']);
  });

  it('steps both ways and wraps at both ends', () => {
    expect([step('session:1', true), step('session:2', false)]).toEqual(['session:2', 'session:1']);
    expect([step('session:3', true), step('session:1', false)]).toEqual(['session:1', 'session:3']);
  });

  it('enters again when the one it was on is gone', () => {
    expect([step('session:8', true), step('session:8', false)]).toEqual(['session:1', 'session:3']);
  });

  it('goes nowhere with nothing to visit', () => {
    expect(stepStop([], null, true)).toBeNull();
  });
});
