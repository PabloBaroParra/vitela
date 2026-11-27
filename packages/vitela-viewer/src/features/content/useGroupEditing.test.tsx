import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { ContentImage, ContentRun, Edit } from '@vitela/core';
import type { Session } from '../annotations/useAnnotationEdits';
import type { ContentGroup, ContentGroupState } from './useContentGroup';
import { useGroupEditing } from './useGroupEditing';

afterEach(cleanup);

const hello: ContentRun = {
  kind: 'run',
  id: 1,
  pageIndex: 0,
  rect: { left: 100, top: 100, width: 40, height: 12 },
  fontName: 'F1',
  fontKind: 'standard14',
  text: 'Hello',
  editable: true,
  pending: false,
};
const photo: ContentImage = { kind: 'image', id: 3, pageIndex: 0, rect: { left: 200, top: 100, width: 100, height: 50 }, inline: false, pending: false };
const info = { pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }], pageIds: [9] };
const session: Session = { info, annotations: [] };
const both: ContentGroup = { pageIndex: 0, key: 'A', items: [hello, photo] };

function setup({ group = both as ContentGroup | null, pending = false, following = false, clipping = false } = {}) {
  const applied: (Edit | null)[] = [];
  const follows: unknown[][] = [];
  let cleared = 0;
  const state = (shown: ContentGroup | null, isFollowing: boolean) =>
    ({
      group: shown,
      following: isFollowing,
      followMoved: (...args: unknown[]) => void follows.push(args),
      clear: () => void cleared++,
    }) as unknown as ContentGroupState;
  const queue = {
    apply: async (build: (session: Session) => Edit | null) => void applied.push(build(session)),
    notify: () => {},
  };
  const hook = renderHook(
    ({ running, shown, isFollowing }: { running: boolean; shown: ContentGroup | null; isFollowing: boolean }) =>
      useGroupEditing({ info, group: state(shown, isFollowing), queue: { ...queue, pending: running }, clipping }),
    { initialProps: { running: pending, shown: group, isFollowing: following } },
  );
  return { hook, applied, follows, cleared: () => cleared };
}

const members = { runIds: [1], imageIds: [3], pathIds: [] };

describe('useGroupEditing', () => {
  it('moves every member by an arrow as one selection edit, and follows the group there', () => {
    const { hook, applied, follows } = setup();
    let used = false;
    act(() => void (used = hook.result.current.onKey('ArrowRight', true)));
    expect(used).toBe(true);
    expect(applied).toEqual([{ kind: 'moveContentSelection', page: 0, members, dx: 10, dy: 0 }]);
    expect(follows).toEqual([[both, 10, 0]]);
    expect(hook.result.current.onKey('Enter', false)).toBe(false);
  });

  it('a drag moves the group; arrows while a step runs add up into one move', () => {
    const { hook, applied } = setup({ pending: true });
    act(() => {
      hook.result.current.move(1, 0);
      hook.result.current.move(0, 2);
    });
    expect(applied).toEqual([]);
    hook.rerender({ running: false, shown: both, isFollowing: false });
    expect(applied).toEqual([{ kind: 'moveContentSelection', page: 0, members, dx: 1, dy: 2 }]);
  });

  it('deletes the group on Delete as one step, and lets it go', () => {
    const { hook, applied, cleared } = setup();
    act(() => void hook.result.current.onKey('Delete', false));
    expect(applied).toEqual([{ kind: 'removeContentSelection', page: 0, members }]);
    expect(cleared()).toBe(1);
  });

  it('deletes nothing while a copy for a cut runs: the cut removes the group itself', () => {
    const { hook, applied, cleared } = setup({ clipping: true });
    act(() => void hook.result.current.onKey('Delete', false));
    act(() => hook.result.current.remove());
    expect(hook.result.current.busy).toBe(true);
    expect(applied).toEqual([]);
    expect(cleared()).toBe(0);
  });

  it('refuses a step whose page changed before its turn', async () => {
    let error: unknown = null;
    const moved: Session = { info: { ...info, pageIds: [10] }, annotations: [] };
    const hook = renderHook(() =>
      useGroupEditing({
        info,
        group: { group: both, following: false, followMoved: () => {}, clear: () => {} } as unknown as ContentGroupState,
        queue: {
          pending: false,
          notify: () => {},
          apply: async (build) => {
            try {
              build(moved);
            } catch (thrown) {
              error = thrown;
            }
          },
        },
      }),
    );
    act(() => void hook.result.current.onKey('Backspace', false));
    await Promise.resolve();
    expect((error as Error).message).toBe('The page changed before the selection could be edited. Nothing was changed.');
  });

  it('takes no key without a group, but an arrow while the moved group is found again', () => {
    const { hook, applied } = setup({ group: null, following: true, pending: true });
    expect(hook.result.current.onKey('Delete', false)).toBe(false);
    expect(hook.result.current.onKey('ArrowLeft', false)).toBe(true);
    hook.rerender({ running: false, shown: both, isFollowing: false });
    expect(applied).toEqual([{ kind: 'moveContentSelection', page: 0, members, dx: -1, dy: 0 }]);
  });
});
