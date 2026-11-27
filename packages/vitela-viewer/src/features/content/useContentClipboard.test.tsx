import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { ContentClip, ContentImage, ContentMembers, Edit, Engine } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { Session } from '../annotations/useAnnotationEdits';
import type { ClipStore } from './clipStore';
import { ContentStatus } from './ContentStatus';
import type { PagePointer } from './selection';
import { useContentClipboard } from './useContentClipboard';
import type { ContentGroup, ContentGroupState } from './useContentGroup';
import type { ContentMode } from './useContentMode';

afterEach(cleanup);

const photo: ContentImage = { kind: 'image', id: 3, pageIndex: 0, rect: { left: 100, top: 200, width: 200, height: 100 }, inline: false, pending: false };
const info = {
  pages: [
    { widthPt: 612, heightPt: 792, rotationDegrees: 0 },
    { widthPt: 612, heightPt: 792, rotationDegrees: 0 },
  ],
  pageIds: [9, 10],
};
const session: Session = { info, annotations: [] };
const clipOf = (fromEncrypted = false): ContentClip => ({
  pdf: new Uint8Array([37, 80, 68, 70]),
  bbox: [100, 492, 200, 100],
  rect: { left: 100, top: 200, width: 200, height: 100 },
  fromEncrypted,
});

interface Options {
  readonly selected?: ContentImage | null;
  readonly group?: ContentGroup | null;
  readonly clip?: ContentClip | null;
  readonly pending?: boolean;
  readonly pointer?: PagePointer | null;
  readonly engine?: Partial<Engine>;
  readonly removal?: 'changed' | 'failed';
}

function setup({ selected = photo, group = null, clip = null, pending = false, pointer = null, engine: overrides = {}, removal = 'changed' }: Options = {}) {
  const copies: [number, ContentMembers, boolean | undefined][] = [];
  const applied: (Edit | null)[] = [];
  const kept: ContentClip[] = [];
  const notices: string[] = [];
  const pasted: unknown[][] = [];
  let cleared = 0;
  const engine = fakeEngine({
    copyContentSelection: async (_document, page, members, forCut) => {
      copies.push([page, members, forCut]);
      return clipOf();
    },
    ...overrides,
  });
  const content = {
    active: true,
    following: false,
    selected: selected && { pageIndex: selected.pageIndex, key: 'A', item: selected },
    clear: () => void cleared++,
  } as unknown as ContentMode;
  const groupState = {
    group,
    following: false,
    clear: () => void cleared++,
    followPasted: (...args: unknown[]) => void pasted.push(args),
  } as unknown as ContentGroupState;
  const queue = {
    pending,
    apply: async (build: (session: Session) => Edit | null) => {
      applied.push(build(session));
      return { status: removal };
    },
    notify: (message: string) => void notices.push(message),
  };
  const store: ClipStore = { clip, keep: (next) => void kept.push(next) };
  const hook = renderHook(() =>
    useContentClipboard({ engine, handle: 1, info, content, group: groupState, queue, store, pointer: () => pointer, visible: { first: 1, last: 1 } }),
  );
  return { hook, copies, applied, kept, notices, pasted, cleared: () => cleared };
}

describe('useContentClipboard', () => {
  it('copies the one item selected into the session’s clip, and says so', async () => {
    const { hook, copies, kept, applied } = setup();
    expect(hook.result.current.canCopy).toBe(true);
    act(() => hook.result.current.copy());
    await waitFor(() => expect(kept).toEqual([clipOf()]));
    expect(copies).toEqual([[0, { runIds: [], imageIds: [3], pathIds: [] }, undefined]]);
    expect(applied).toEqual([]);
    await waitFor(() => expect(hook.result.current.message).toBe('Copied 1 item.'));
  });

  it('copies the group when there is one', async () => {
    const run = { ...photo, kind: 'run', id: 7, fontName: 'F1', fontKind: 'standard14', text: 'Hi', editable: true } as const;
    const { hook, copies } = setup({ selected: null, group: { pageIndex: 1, key: 'B', items: [run, { ...photo, pageIndex: 1 }] } });
    act(() => hook.result.current.copy());
    await waitFor(() => expect(copies).toEqual([[1, { runIds: [7], imageIds: [3], pathIds: [] }, undefined]]));
    await waitFor(() => expect(hook.result.current.message).toBe('Copied 2 items.'));
  });

  it('says the core’s refusal of a copy, and keeps no clip', async () => {
    const { hook, kept, notices } = setup({
      engine: {
        copyContentSelection: async () => {
          throw Object.assign(new Error('this document does not permit copying its text'), { name: 'TextExtractionDenied' });
        },
      },
    });
    act(() => hook.result.current.copy());
    await waitFor(() => expect(notices).toEqual(['This document does not permit copying its text.']));
    expect(kept).toEqual([]);
  });

  it('cuts: the copy for a cut, then the removal as one step; the clip is kept once the removal is in', async () => {
    const { hook, copies, applied, kept, cleared } = setup();
    act(() => hook.result.current.cut());
    await waitFor(() => expect(kept).toEqual([clipOf()]));
    expect(copies).toEqual([[0, { runIds: [], imageIds: [3], pathIds: [] }, true]]);
    expect(applied).toEqual([{ kind: 'removeContentSelection', page: 0, members: { runIds: [], imageIds: [3], pathIds: [] } }]);
    expect(cleared()).toBeGreaterThan(0);
    await waitFor(() => expect(hook.result.current.message).toBe('Cut 1 item.'));
  });

  it('is busy while the copy for a cut runs: a second cut asks nothing, and its key is left to the browser', async () => {
    let asked = 0;
    let release: (clip: ContentClip) => void = () => {};
    const { hook, applied, kept } = setup({
      clip: clipOf(),
      engine: {
        copyContentSelection: () => {
          asked++;
          return new Promise<ContentClip>((resolve) => (release = resolve));
        },
      },
    });
    act(() => {
      hook.result.current.cut();
      hook.result.current.cut();
    });
    expect(hook.result.current).toMatchObject({ clipping: true, canCopy: false, canPaste: false });
    const again = new KeyboardEvent('keydown', { key: 'x', ctrlKey: true, cancelable: true, bubbles: true });
    act(() => void window.dispatchEvent(again));
    expect(again.defaultPrevented).toBe(false);
    expect(asked).toBe(1);
    await act(async () => release(clipOf()));
    await waitFor(() => expect(kept).toEqual([clipOf()]));
    expect(applied).toHaveLength(1);
    expect(hook.result.current.clipping).toBe(false);
  });

  it('keeps no clip when a cut’s removal is refused', async () => {
    const { hook, applied, kept } = setup({ removal: 'failed' });
    act(() => hook.result.current.cut());
    await waitFor(() => expect(applied).toHaveLength(1));
    await Promise.resolve();
    expect(kept).toEqual([]);
  });

  it('pastes at the pointer over a page, and follows what it adds there', async () => {
    const clip = clipOf(true);
    const { hook, applied, pasted } = setup({ clip, pointer: { pageIndex: 0, point: { x: 40, y: 50 } } });
    expect(hook.result.current.canPaste).toBe(true);
    act(() => hook.result.current.paste(true));
    // The clip goes back as the core handed it out, its provenance included.
    expect(applied).toEqual([{ kind: 'pasteContent', page: 0, clip, at: { x: 40, y: 50 } }]);
    expect(pasted).toEqual([[0, { left: 40, top: 50, width: 200, height: 100 }]]);
    await waitFor(() => expect(hook.result.current.message).toBe('Pasted on page 1. Drag it into place.'));
  });

  it('pastes from the button where the clip was copied from, on the first page in view', () => {
    const { hook, applied } = setup({ clip: clipOf(), pointer: { pageIndex: 0, point: { x: 40, y: 50 } } });
    act(() => hook.result.current.paste());
    expect(applied).toEqual([{ kind: 'pasteContent', page: 1, clip: clipOf(), at: { x: 100, y: 200 } }]);
  });

  it('offers nothing while a step runs, with nothing selected, or with no clip', () => {
    expect(setup({ pending: true, clip: clipOf() }).hook.result.current).toMatchObject({ canCopy: false, canPaste: false });
    expect(setup({ selected: null }).hook.result.current).toMatchObject({ canCopy: false, canPaste: false });
  });

  it('answers Ctrl/⌘ + C and V on the page, never in a text field, and leaves a key it has nothing for to the browser', async () => {
    const { hook, applied, copies } = setup({ clip: clipOf(), pointer: { pageIndex: 0, point: { x: 1, y: 2 } } });
    const copyKey = new KeyboardEvent('keydown', { key: 'c', ctrlKey: true, cancelable: true, bubbles: true });
    act(() => void window.dispatchEvent(copyKey));
    expect(copyKey.defaultPrevented).toBe(true);
    await waitFor(() => expect(copies).toHaveLength(1));
    // A paste waits for the copy to be read.
    await waitFor(() => expect(hook.result.current.clipping).toBe(false));

    const field = document.body.appendChild(document.createElement('input'));
    const typed = new KeyboardEvent('keydown', { key: 'v', ctrlKey: true, cancelable: true, bubbles: true });
    act(() => void field.dispatchEvent(typed));
    expect(typed.defaultPrevented).toBe(false);
    expect(applied).toEqual([]);
    field.remove();

    const pasteKey = new KeyboardEvent('keydown', { key: 'v', metaKey: true, cancelable: true, bubbles: true });
    act(() => void window.dispatchEvent(pasteKey));
    expect(pasteKey.defaultPrevented).toBe(true);
    expect(applied).toEqual([{ kind: 'pasteContent', page: 0, clip: clipOf(), at: { x: 1, y: 2 } }]);
    cleanup();

    setup({ selected: null });
    const nothing = new KeyboardEvent('keydown', { key: 'x', ctrlKey: true, cancelable: true, bubbles: true });
    act(() => void window.dispatchEvent(nothing));
    expect(nothing.defaultPrevented).toBe(false);
  });
});

describe('ContentStatus clipboard buttons', () => {
  it('copies, cuts and pastes, each enabled only when it may be pressed', () => {
    const pressed: string[] = [];
    const clipboard = (canCopy: boolean, canPaste: boolean) => ({
      canCopy,
      canPaste,
      onCopy: () => void pressed.push('copy'),
      onCut: () => void pressed.push('cut'),
      onPaste: () => void pressed.push('paste'),
    });
    const { rerender } = render(<ContentStatus status="x" onDone={() => {}} clipboard={clipboard(false, false)} />);
    const group = screen.getByRole('group', { name: 'Clipboard' });
    for (const name of ['Copy', 'Cut', 'Paste']) expect(screen.getByRole('button', { name }).hasAttribute('disabled')).toBe(true);
    rerender(<ContentStatus status="x" onDone={() => {}} clipboard={clipboard(true, true)} />);
    for (const name of ['Copy', 'Cut', 'Paste']) fireEvent.click(screen.getByRole('button', { name }));
    expect(pressed).toEqual(['copy', 'cut', 'paste']);
    expect(group).toBeTruthy();
  });
});
