import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { ContentClip, ContentPath, Edit, PageContent } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { Session } from '../annotations/useAnnotationEdits';
import type { OpenDocument } from '../document/useDocument';
import { useContentEditing } from './ContentEditing';
import { heldWhileClipping } from './contentEdits';

afterEach(cleanup);

const shape: ContentPath = { kind: 'path', id: 2, pageIndex: 0, rect: { left: 300, top: 442, width: 100, height: 50 }, paint: 'fill', pending: false };
const page: PageContent = { runs: [], images: [], paths: [shape] };
const info = { pageCount: 1, pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }], pageIds: [9], textExtractionAllowed: true };
const document = { name: 'shapes.pdf', info, handle: 1, generation: 1 } as unknown as OpenDocument;
const session = { info, annotations: [] } as unknown as Session;
const clip: ContentClip = { pdf: new Uint8Array([37]), bbox: [300, 300, 100, 50], rect: shape.rect, fromEncrypted: false };

/** The mode on one page holding one filled box, with a copy for a cut that answers only when the test says so. */
function setup() {
  const applied: (Edit | null)[] = [];
  let answerCopy: () => void = () => {};
  const engine = fakeEngine({
    readPageContent: async () => page,
    copyContentSelection: () =>
      new Promise<ContentClip>((resolve) => {
        answerCopy = () => resolve(clip);
      }),
  });
  const queue = {
    pending: false,
    apply: async (build: (session: Session) => Edit | null) => {
      applied.push(build(session));
      return { status: 'changed' };
    },
    notify: () => {},
    settled: async () => {},
  };
  const hook = renderHook(() =>
    useContentEditing({ engine, handle: 1, document, active: true, queue, visible: { first: 0, last: 0 }, focusPages: () => {} }),
  );
  const answer = async () => {
    await act(async () => answerCopy());
  };
  return { hook, applied, answer };
}

async function selectShape(hook: ReturnType<typeof setup>['hook']) {
  await waitFor(() => expect(hook.result.current.content.stateAt(0)?.status).toBe('ready'));
  act(() => hook.result.current.input!.onSelect(0, { x: 350, y: 460 }));
  expect(hook.result.current.paths.path).toMatchObject({ id: 2 });
}

describe('useContentEditing', () => {
  it('moves the selected path with an arrow', async () => {
    const { hook, applied } = setup();
    await selectShape(hook);
    act(() => void hook.result.current.onKey('ArrowRight', false));
    expect(applied).toEqual([{ kind: 'movePath', page: 0, pathId: 2, dx: 1, dy: 0 }]);
  });

  it('deletes the selected path with Delete', async () => {
    const { hook, applied } = setup();
    await selectShape(hook);
    act(() => void hook.result.current.onKey('Delete', false));
    expect(applied).toEqual([{ kind: 'removePath', page: 0, pathId: 2 }]);
  });

  it('drags the selected path by the distance it was let go at', async () => {
    const { hook, applied } = setup();
    await selectShape(hook);
    act(() => hook.result.current.input!.onDrag(0, 12, -4));
    expect(applied).toEqual([{ kind: 'movePath', page: 0, pathId: 2, dx: 12, dy: -4 }]);
  });

  it('while a cut is read, takes Delete, Backspace and the arrows and edits nothing; other keys go on', async () => {
    const { hook, applied, answer } = setup();
    await selectShape(hook);
    act(() => hook.result.current.clipboard.cut());
    expect(hook.result.current.clipboard.clipping).toBe(true);

    for (const key of ['Delete', 'Backspace', 'ArrowLeft', 'ArrowDown']) expect(hook.result.current.onKey(key, false)).toBe(true);
    // Not swallowed: nothing here uses them, so they are the browser's.
    expect(hook.result.current.onKey('Enter', false)).toBe(false);
    expect(hook.result.current.onKey('a', false)).toBe(false);

    // The overlay and its toolbar are busy, and asking anyway records nothing.
    expect(hook.result.current.paths.busy).toBe(true);
    expect(hook.result.current.images.busy).toBe(true);
    expect(hook.result.current.textRuns.busy).toBe(true);
    act(() => {
      hook.result.current.paths.remove();
      hook.result.current.paths.resize({ left: 0, top: 0, width: 10, height: 10 });
      hook.result.current.input!.onDrag(0, 5, 5);
      hook.result.current.input!.onOpen(0, { x: 350, y: 460 });
    });
    expect(applied).toEqual([]);

    // The cut's own removal is the one step.
    await answer();
    expect(applied).toEqual([{ kind: 'removeContentSelection', page: 0, members: { runIds: [], imageIds: [], pathIds: [2] } }]);
    await waitFor(() => expect(hook.result.current.clipboard.clipping).toBe(false));
  });
});

describe('heldWhileClipping', () => {
  it('holds the keys that would edit the selection, and only those', () => {
    for (const key of ['Delete', 'Backspace', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight']) expect(heldWhileClipping(key)).toBe(true);
    for (const key of ['Escape', 'Enter', 'Tab', 'a', ' ']) expect(heldWhileClipping(key)).toBe(false);
  });
});
