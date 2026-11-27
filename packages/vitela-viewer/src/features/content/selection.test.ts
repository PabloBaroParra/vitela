import { describe, expect, it } from 'vitest';
import type { ContentClip, ContentImage, ContentPath, ContentRun, PageContent } from '@vitela/core';
import {
  boundsOf,
  clipboardAction,
  describeGroup,
  followGroup,
  gathered,
  marqueeRect,
  membersOf,
  pastedItems,
  pastedRect,
  pastePoint,
  soughtOf,
  toggled,
} from './selection';

const run = (id: number, left: number, top: number, text = 'Hello', pending = false): ContentRun => ({
  kind: 'run',
  id,
  pageIndex: 0,
  rect: { left, top, width: 40, height: 12 },
  fontName: 'F1',
  fontKind: 'standard14',
  text,
  editable: true,
  pending,
});
const image = (id: number, left: number, top: number, pending = false): ContentImage => ({
  kind: 'image',
  id,
  pageIndex: 0,
  rect: { left, top, width: 100, height: 50 },
  inline: false,
  pending,
});
const rule = (id: number, left: number, top: number): ContentPath => ({
  kind: 'path',
  id,
  pageIndex: 0,
  rect: { left, top, width: 200, height: 0 },
  paint: 'stroke',
  pending: false,
});
const page = (items: { runs?: ContentRun[]; images?: ContentImage[]; paths?: ContentPath[] }): PageContent => ({
  runs: items.runs ?? [],
  images: items.images ?? [],
  paths: items.paths ?? [],
});

describe('membersOf', () => {
  it('sorts the items into the ids the core takes, by kind', () => {
    expect(membersOf([run(3, 0, 0), image(1, 0, 0), rule(7, 0, 0), run(4, 0, 0)])).toEqual({
      runIds: [3, 4],
      imageIds: [1],
      pathIds: [7],
    });
  });
});

describe('toggled', () => {
  it('adds an item that is not there and takes out one that is', () => {
    const a = run(1, 0, 0);
    const b = image(2, 0, 0);
    expect(toggled([a], b)).toEqual([a, b]);
    expect(toggled([a, b], { ...a })).toEqual([b]);
  });
});

describe('marqueeRect and gathered', () => {
  it('spans two corners whichever way the drag went', () => {
    expect(marqueeRect({ x: 200, y: 50 }, { x: 100, y: 150 })).toEqual({ left: 100, top: 50, width: 100, height: 100 });
  });

  it('gathers every item the box touches, as the Windows shell does, a hairline rule included', () => {
    const content = page({ runs: [run(1, 100, 100), run(2, 400, 400)], images: [image(3, 180, 130)], paths: [rule(4, 50, 160)] });
    const box = marqueeRect({ x: 90, y: 90 }, { x: 200, y: 160 });
    expect(gathered(content, box).map((item) => `${item.kind}:${item.id}`)).toEqual(['run:1', 'image:3', 'path:4']);
  });
});

describe('boundsOf', () => {
  it('is the union of the boxes', () => {
    expect(boundsOf([run(1, 100, 100), image(2, 120, 50)])).toEqual({ left: 100, top: 50, width: 120, height: 62 });
    expect(boundsOf([])).toBeNull();
  });
});

describe('followGroup', () => {
  it('finds each moved item again by kind and place, never the same one twice', () => {
    const moved = soughtOf([run(1, 100, 100), run(2, 100, 120, 'World'), image(3, 300, 300)], 10, 5);
    expect(moved).toEqual([
      { kind: 'run', rect: { left: 110, top: 105, width: 40, height: 12 }, text: 'Hello' },
      { kind: 'run', rect: { left: 110, top: 125, width: 40, height: 12 }, text: 'World' },
      { kind: 'image', rect: { left: 310, top: 305, width: 100, height: 50 } },
    ]);
    const after = page({ runs: [run(7, 110.2, 105), run(8, 110, 125, 'World'), run(9, 400, 400)], images: [image(5, 310, 305)] });
    expect(followGroup(after, moved).map((item) => item.id)).toEqual([7, 8, 5]);
  });

  it('leaves out what it cannot find', () => {
    const moved = soughtOf([image(3, 300, 300), rule(4, 50, 50)], 0, 0);
    expect(followGroup(page({ paths: [rule(9, 50, 50)] }), moved).map((item) => item.id)).toEqual([9]);
  });
});

describe('pastedRect and pastedItems', () => {
  const clip: ContentClip = {
    pdf: new Uint8Array(),
    bbox: [72, 500, 100, 50],
    rect: { left: 72, top: 242, width: 100, height: 50 },
    fromEncrypted: false,
  };

  it('hangs the clip from its top-left corner, turned with the page about it', () => {
    expect(pastedRect({ x: 300, y: 200 }, clip, 0)).toEqual({ left: 300, top: 200, width: 100, height: 50 });
    expect(pastedRect({ x: 300, y: 200 }, clip, 90)).toEqual({ left: 250, top: 200, width: 50, height: 100 });
    expect(pastedRect({ x: 300, y: 200 }, clip, 180)).toEqual({ left: 200, top: 150, width: 100, height: 50 });
    expect(pastedRect({ x: 300, y: 200 }, clip, 270)).toEqual({ left: 300, top: 100, width: 50, height: 100 });
  });

  it('selects only what a pending edit added where the paste landed', () => {
    const content = page({
      runs: [run(1, 300, 200, 'Hello', true), run(2, 310, 210, 'Old'), run(3, 20, 20, 'Elsewhere', true)],
      images: [image(4, 300, 220, true)],
    });
    expect(pastedItems(content, { left: 300, top: 200, width: 100, height: 70 }).map((item) => item.id)).toEqual([1, 4]);
  });

  it('selects only what this paste added: not an earlier paste under it, nor an unsaved insert it covers', () => {
    const landed = { left: 300, top: 200, width: 100, height: 70 };
    const first = page({ runs: [run(1, 300, 200, 'Hello', true)], images: [image(2, 300, 220, true)] });
    const stacked = page({
      runs: [run(5, 300, 200, 'Hello', true), run(7, 300, 200, 'Hello', true)],
      images: [image(6, 300, 220, true), image(8, 300, 220, true)],
    });
    expect(pastedItems(stacked, landed, first).map((item) => item.id)).toEqual([7, 8]);
    const typed = page({ runs: [run(1, 320, 210, 'Typed', true)] });
    const over = page({ runs: [run(3, 320, 210, 'Typed', true), run(4, 300, 200, 'Hello', true)] });
    expect(pastedItems(over, landed, typed).map((item) => item.id)).toEqual([4]);
  });
});

describe('pastePoint', () => {
  const clip: ContentClip = {
    pdf: new Uint8Array(),
    bbox: [72, 500, 100, 50],
    rect: { left: 72, top: 242, width: 100, height: 50 },
    fromEncrypted: false,
  };
  const pages = [
    { widthPt: 612, heightPt: 792 },
    { widthPt: 300, heightPt: 200 },
  ];

  it('pastes at the pointer when it is over a page', () => {
    expect(pastePoint(clip, { pageIndex: 1, point: { x: 30, y: 40 } }, 0, pages)).toEqual({ pageIndex: 1, at: { x: 30, y: 40 } });
  });

  it('else at the place it was copied from, on the first page in view, kept on the page', () => {
    expect(pastePoint(clip, null, 0, pages)).toEqual({ pageIndex: 0, at: { x: 72, y: 242 } });
    expect(pastePoint(clip, null, 1, pages)).toEqual({ pageIndex: 1, at: { x: 72, y: 150 } });
    expect(pastePoint(clip, null, 7, pages)).toBeNull();
  });
});

describe('describeGroup', () => {
  it('counts what is selected on its page', () => {
    expect(describeGroup(3, 1)).toBe('3 items selected on page 2. Drag them, press Delete, or copy them.');
    expect(describeGroup(1, 0)).toBe('1 item selected on page 1. Drag it, press Delete, or copy it.');
  });
});

describe('clipboardAction', () => {
  const key = (init: KeyboardEventInit, target: EventTarget | null = null) => {
    const event = new KeyboardEvent('keydown', init);
    if (target) Object.defineProperty(event, 'target', { value: target });
    return event;
  };

  it('takes Ctrl or Cmd with C, X or V', () => {
    expect(clipboardAction(key({ key: 'c', ctrlKey: true }), null)).toBe('copy');
    expect(clipboardAction(key({ key: 'X', metaKey: true }), null)).toBe('cut');
    expect(clipboardAction(key({ key: 'v', ctrlKey: true }), null)).toBe('paste');
    expect(clipboardAction(key({ key: 'v' }), null)).toBeNull();
    expect(clipboardAction(key({ key: 'v', ctrlKey: true, shiftKey: true }), null)).toBeNull();
    expect(clipboardAction(key({ key: 'z', ctrlKey: true }), null)).toBeNull();
  });

  it('leaves a text field, and text the browser has selected, alone', () => {
    const field = document.createElement('textarea');
    expect(clipboardAction(key({ key: 'v', ctrlKey: true }, field), null)).toBeNull();
    const selected = { isCollapsed: false } as Selection;
    expect(clipboardAction(key({ key: 'c', ctrlKey: true }), selected)).toBeNull();
    // A paste replaces no browser selection: it is still ours.
    expect(clipboardAction(key({ key: 'v', ctrlKey: true }), selected)).toBe('paste');
  });
});
