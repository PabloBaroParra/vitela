import { describe, expect, it } from 'vitest';
import type { ContentImage, ContentPath, ContentRun, PageContent } from '@vitela/core';
import {
  contentKey,
  describeItem,
  itemAt,
  keepWanted,
  pagesToParse,
  readsToSend,
  sameItem,
  type PageContentState,
} from './contentModel';

const run = (id: number, left: number, top: number, text = 'Hello', editable = true): ContentRun => ({
  kind: 'run',
  id,
  pageIndex: 0,
  rect: { left, top, width: 40, height: 12 },
  fontName: 'F1',
  fontKind: 'standard14',
  text,
  editable,
});
const image: ContentImage = {
  kind: 'image',
  id: 0,
  pageIndex: 0,
  rect: { left: 50, top: 50, width: 300, height: 300 },
  inline: false,
};
const rule: ContentPath = {
  kind: 'path',
  id: 0,
  pageIndex: 0,
  rect: { left: 50, top: 500, width: 200, height: 0 },
  paint: 'stroke',
};
const page: PageContent = { runs: [run(0, 100, 100), run(1, 400, 400)], images: [image], paths: [rule] };

describe('pagesToParse', () => {
  it('takes the visible pages and one on each side, inside the document', () => {
    expect(pagesToParse({ first: 3, last: 4 }, 10)).toEqual([2, 3, 4, 5]);
    expect(pagesToParse({ first: 0, last: 0 }, 10)).toEqual([0, 1]);
    expect(pagesToParse({ first: 9, last: 9 }, 10)).toEqual([8, 9]);
    expect(pagesToParse({ first: 0, last: 0 }, 1)).toEqual([0]);
  });

  it('parses nothing when nothing is visible, and never the whole of a long document', () => {
    expect(pagesToParse(null, 10)).toEqual([]);
    expect(pagesToParse({ first: 0, last: 0 }, 0)).toEqual([]);
    expect(pagesToParse({ first: 800, last: 801 }, 1619)).toHaveLength(4);
  });
});

describe('contentKey and keepWanted', () => {
  it('keys a parse on its page id, turn, paint revision and base generation', () => {
    // An undo of a rotation made while the mode is on turns the page under
    // the same id and revision: its outlines are elsewhere.
    expect(contentKey(7, 90, 0, 0)).not.toBe(contentKey(7, 0, 0, 0));
    const keys = new Set([contentKey(7, 0, 0, 0), contentKey(7, 0, 1, 0), contentKey(7, 0, 0, 1), contentKey(8, 0, 0, 0)]);
    expect(keys.size).toBe(4);
    expect(contentKey(7, 0, 2, 1)).toBe(contentKey(7, 0, 2, 1));
  });

  it('keeps only the parses still wanted', () => {
    const ready: PageContentState = { status: 'ready', content: page };
    const held = new Map<string, PageContentState>([
      [contentKey(1, 0, 0, 0), ready],
      [contentKey(2, 0, 0, 0), { status: 'loading' }],
      [contentKey(3, 0, 0, 0), ready],
    ]);
    const kept = keepWanted(held, new Set([contentKey(1, 0, 0, 0), contentKey(3, 0, 1, 0)]));
    expect([...kept.keys()]).toEqual([contentKey(1, 0, 0, 0)]);
    expect(kept.get(contentKey(1, 0, 0, 0))).toBe(ready);
  });
});

describe('readsToSend', () => {
  const loading: PageContentState = { status: 'loading' };
  const ready: PageContentState = { status: 'ready', content: page };
  const wanted = [
    { index: 4, key: 'a' },
    { index: 5, key: 'b' },
    { index: 6, key: 'c' },
  ];

  it('sends the wanted pages still waiting, in order, no more than the limit at once', () => {
    const held = new Map<string, PageContentState>([
      ['a', ready],
      ['b', loading],
      ['c', loading],
    ]);
    expect(readsToSend(wanted, held, new Set(), 1)).toEqual([{ index: 5, key: 'b' }]);
    expect(readsToSend(wanted, held, new Set(), 2)).toEqual([
      { index: 5, key: 'b' },
      { index: 6, key: 'c' },
    ]);
  });

  it('never asks again for a page whose read is already in flight', () => {
    const held = new Map([
      ['a', loading],
      ['b', loading],
      ['c', loading],
    ]);
    expect(readsToSend(wanted, held, new Set(['a']), 2)).toEqual([{ index: 5, key: 'b' }]);
    expect(readsToSend(wanted, held, new Set(['a', 'b']), 2)).toEqual([]);
  });
});

describe('itemAt', () => {
  it('finds the run over the image rather than the image', () => {
    expect(itemAt(page, { x: 110, y: 105 })).toBe(page.runs[0]);
    expect(itemAt(page, { x: 200, y: 200 })).toBe(image);
  });

  it('finds a rule with no height within the slop, and nothing beside it', () => {
    expect(itemAt(page, { x: 120, y: 502 })).toBe(rule);
    expect(itemAt(page, { x: 120, y: 510 })).toBeNull();
    expect(itemAt(page, { x: 600, y: 700 })).toBeNull();
  });
});

describe('sameItem', () => {
  it('compares kind, id and page', () => {
    expect(sameItem(run(0, 0, 0), run(0, 10, 10, 'moved'))).toBe(true);
    expect(sameItem(run(0, 0, 0), { ...image, id: 0 })).toBe(false);
    expect(sameItem(run(0, 0, 0), { ...run(0, 0, 0), pageIndex: 1 })).toBe(false);
    expect(sameItem(null, run(0, 0, 0))).toBe(false);
  });
});

describe('describeItem', () => {
  it('names a run by its text, an image and a shape by their kind, with the page', () => {
    expect(describeItem(run(0, 0, 0, ' Hello world '))).toBe('Text “Hello world” selected on page 1.');
    expect(describeItem({ ...image, pageIndex: 2 })).toBe('Image selected on page 3.');
    expect(describeItem(rule)).toBe('Shape selected on page 1.');
  });

  it('says why a run whose text did not decode cannot be retyped', () => {
    expect(describeItem(run(0, 0, 0, 'A�', false))).toMatch(/cannot be retyped\.$/);
  });
});
