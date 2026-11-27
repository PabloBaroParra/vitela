import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { ContentImage, ContentItem, ContentRun, PageContent, PagePoint } from '@vitela/core';
import type { PageContentState } from './contentModel';
import { itemAt } from './contentModel';
import { useContentGroup } from './useContentGroup';
import type { ContentMode, Picked } from './useContentMode';

afterEach(cleanup);

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
const page = (runs: ContentRun[], images: ContentImage[] = []): PageContent => ({ runs, images, paths: [] });

interface Props {
  readonly key: string;
  readonly state: PageContentState | undefined;
  readonly paused: boolean;
  readonly selected?: Picked | null;
}

function setup(content: PageContent) {
  const chosen: (Picked | null)[] = [];
  let cleared = 0;
  const mode = ({ key, state, selected = null }: Props) =>
    ({
      active: true,
      selected,
      keyAt: () => key,
      stateAt: () => state,
      wants: () => true,
      itemAt: (pageIndex: number, point: PagePoint): Picked | null => {
        if (state?.status !== 'ready') return null;
        const item = itemAt(state.content, point);
        return item && { pageIndex, key, item };
      },
      choose: (picked: Picked | null) => void chosen.push(picked),
      clear: () => void cleared++,
    }) as unknown as ContentMode;
  const hook = renderHook((props: Props) => useContentGroup({ content: mode(props), paused: props.paused }), {
    initialProps: { key: 'A', state: { status: 'ready', content }, paused: false } as Props,
  });
  return { hook, chosen, cleared: () => cleared };
}

const ids = (items: readonly ContentItem[] | undefined) => items?.map((item) => `${item.kind}:${item.id}`);

describe('useContentGroup', () => {
  it('a toggle adds the item under the pointer to the one selected, and taking one out leaves it the single selection', () => {
    const hello = run(1, 100, 100);
    const photo = image(2, 300, 300);
    const { hook, chosen } = setup(page([hello], [photo]));
    hook.rerender({ key: 'A', state: { status: 'ready', content: page([hello], [photo]) }, paused: false, selected: { pageIndex: 0, key: 'A', item: hello } });
    act(() => hook.result.current.toggle(0, { x: 310, y: 310 }));
    expect(ids(hook.result.current.group?.items)).toEqual(['run:1', 'image:2']);
    expect(hook.result.current.status).toBe('2 items selected on page 1. Drag them, press Delete, or copy them.');
    act(() => hook.result.current.toggle(0, { x: 110, y: 105 }));
    expect(hook.result.current.group).toBeNull();
    expect(chosen.at(-1)).toEqual({ pageIndex: 0, key: 'A', item: photo });
    // Over nothing a toggle does nothing.
    act(() => hook.result.current.toggle(0, { x: 5, y: 5 }));
    expect(chosen).toHaveLength(1);
  });

  it('a marquee selects what it touches: several as a group, one as the single selection, none as nothing', () => {
    const content = page([run(1, 100, 100), run(2, 100, 400)], [image(3, 160, 90)]);
    const { hook, chosen, cleared } = setup(content);
    act(() => hook.result.current.gather(0, { left: 90, top: 90, width: 200, height: 40 }, false));
    expect(ids(hook.result.current.group?.items)).toEqual(['run:1', 'image:3']);
    act(() => hook.result.current.gather(0, { left: 90, top: 395, width: 30, height: 20 }, true));
    expect(ids(hook.result.current.group?.items)).toEqual(['run:1', 'image:3', 'run:2']);
    act(() => hook.result.current.gather(0, { left: 90, top: 395, width: 30, height: 20 }, false));
    expect(hook.result.current.group).toBeNull();
    expect(chosen.at(-1)?.item).toMatchObject({ kind: 'run', id: 2 });
    const before = cleared();
    act(() => hook.result.current.gather(0, { left: 500, top: 500, width: 10, height: 10 }, false));
    expect(hook.result.current.group).toBeNull();
    expect(cleared()).toBeGreaterThan(before);
  });

  it('holds only while its page shows the read it was made in', () => {
    const content = page([run(1, 100, 100)], [image(3, 160, 90)]);
    const { hook } = setup(content);
    act(() => hook.result.current.gather(0, { left: 90, top: 90, width: 200, height: 40 }, false));
    expect(hook.result.current.group).not.toBeNull();
    hook.rerender({ key: 'B', state: { status: 'loading' }, paused: false });
    expect(hook.result.current.group).toBeNull();
  });

  it('finds a moved group again where the move put it, once its page is read anew', () => {
    const content = page([run(1, 100, 100)], [image(3, 160, 90)]);
    const { hook } = setup(content);
    act(() => hook.result.current.gather(0, { left: 90, top: 90, width: 200, height: 40 }, false));
    act(() => hook.result.current.followMoved(hook.result.current.group!, 10, 0));
    expect(hook.result.current.following).toBe(true);
    hook.rerender({ key: 'A', state: undefined, paused: true });
    hook.rerender({ key: 'B', state: { status: 'loading' }, paused: false });
    expect(hook.result.current.following).toBe(true);
    const moved = page([run(7, 110, 100), run(8, 400, 400)], [image(9, 170, 90)]);
    hook.rerender({ key: 'B', state: { status: 'ready', content: moved }, paused: false });
    expect(hook.result.current.following).toBe(false);
    expect(ids(hook.result.current.group?.items)).toEqual(['run:7', 'image:9']);
  });

  it('selects what a paste added where it landed: a group, or one image alone as a click would', () => {
    const { hook, chosen } = setup(page([]));
    act(() => hook.result.current.followPasted(0, { left: 300, top: 200, width: 100, height: 70 }));
    hook.rerender({ key: 'A', state: undefined, paused: true });
    const pasted = page([run(1, 300, 200, 'Hello', true), run(2, 310, 210, 'Old')], [image(4, 300, 220, true)]);
    hook.rerender({ key: 'B', state: { status: 'ready', content: pasted }, paused: false });
    expect(ids(hook.result.current.group?.items)).toEqual(['run:1', 'image:4']);

    act(() => hook.result.current.followPasted(0, { left: 300, top: 220, width: 100, height: 50 }));
    hook.rerender({ key: 'B', state: undefined, paused: true });
    // The earlier paste is still there, stacked under this one: only this one's image is selected.
    const again = page([run(1, 300, 200, 'Hello', true), run(2, 310, 210, 'Old')], [image(4, 300, 220, true), image(5, 300, 220, true)]);
    hook.rerender({ key: 'C', state: { status: 'ready', content: again }, paused: false });
    expect(hook.result.current.group).toBeNull();
    expect(chosen.at(-1)).toEqual({ pageIndex: 0, key: 'C', item: image(5, 300, 220, true) });
  });

  it('a refused edit leaves the group as it was', () => {
    const content = page([run(1, 100, 100)], [image(3, 160, 90)]);
    const { hook } = setup(content);
    act(() => hook.result.current.gather(0, { left: 90, top: 90, width: 200, height: 40 }, false));
    act(() => hook.result.current.followMoved(hook.result.current.group!, 10, 0));
    hook.rerender({ key: 'A', state: undefined, paused: true });
    hook.rerender({ key: 'A', state: { status: 'ready', content }, paused: false });
    expect(hook.result.current.following).toBe(false);
    expect(ids(hook.result.current.group?.items)).toEqual(['run:1', 'image:3']);
  });
});
