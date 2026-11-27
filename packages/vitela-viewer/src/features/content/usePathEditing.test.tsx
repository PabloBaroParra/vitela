import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ContentPath, Edit } from '@vitela/core';
import type { Session } from '../annotations/useAnnotationEdits';
import { PathOverlay } from './PathOverlay';
import type { ContentMode } from './useContentMode';
import { usePathEditing } from './usePathEditing';

afterEach(cleanup);

const shape: ContentPath = { kind: 'path', id: 4, pageIndex: 0, rect: { left: 100, top: 200, width: 200, height: 100 }, paint: 'fill', pending: false };
const rule: ContentPath = { kind: 'path', id: 5, pageIndex: 0, rect: { left: 72, top: 392, width: 200, height: 0 }, paint: 'stroke', pending: false };
const info = { pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }], pageIds: [9] };
const session: Session = { info, annotations: [] };

function setup({ path = shape as ContentPath | null, pending = false, following = null as 'path' | 'image' | null, clipping = false } = {}) {
  const applied: (Edit | null)[] = [];
  const follows: unknown[][] = [];
  const content = (selected: ContentPath | null) =>
    ({
      active: true,
      selected: selected && { pageIndex: selected.pageIndex, key: '9/0/0/0', item: selected },
      following: following !== null,
      followingKind: following,
      followPath: (...args: unknown[]) => void follows.push(args),
    }) as unknown as ContentMode;
  const queue = {
    pending,
    apply: async (build: (session: Session) => Edit | null) => void applied.push(build(session)),
    notify: () => {},
  };
  const hook = renderHook(
    ({ running, selected, cutting }: { running: boolean; selected: ContentPath | null; cutting: boolean }) =>
      usePathEditing({ info, content: content(selected), queue: { ...queue, pending: running }, clipping: cutting }),
    { initialProps: { running: pending, selected: path, cutting: clipping } },
  );
  return { hook, applied, follows };
}

describe('usePathEditing', () => {
  it('moves the selected path by an arrow and follows it to its new place', () => {
    const { hook, applied, follows } = setup();
    let used = false;
    act(() => void (used = hook.result.current.onKey('ArrowLeft', true)));
    expect(used).toBe(true);
    expect(applied).toEqual([{ kind: 'movePath', page: 0, pathId: 4, dx: -10, dy: 0 }]);
    expect(follows).toEqual([[0, { ...shape.rect, left: 90 }]]);
    expect(hook.result.current.onKey('Enter', false)).toBe(false);
  });

  it('adds up the arrows pressed while a step runs into one move once it is done', () => {
    const { hook, applied } = setup({ pending: true });
    act(() => {
      hook.result.current.move(0, 1);
      hook.result.current.move(0, 1);
    });
    expect(applied).toEqual([]);
    hook.rerender({ running: false, selected: shape, cutting: false });
    expect(applied).toEqual([{ kind: 'movePath', page: 0, pathId: 4, dx: 0, dy: 2 }]);
  });

  it('resizes to a box and follows it there; the box it has asks for nothing', () => {
    const { hook, applied, follows } = setup();
    const rect = { left: 100, top: 200, width: 300, height: 150 };
    act(() => hook.result.current.resize(rect));
    expect(applied).toEqual([{ kind: 'resizePath', page: 0, pathId: 4, rect }]);
    expect(follows).toEqual([[0, rect]]);
    act(() => hook.result.current.resize(shape.rect));
    expect(applied).toHaveLength(1);
  });

  it('deletes on Delete or Backspace and follows nothing', () => {
    const { hook, applied, follows } = setup();
    act(() => void hook.result.current.onKey('Backspace', false));
    expect(applied).toEqual([{ kind: 'removePath', page: 0, pathId: 4 }]);
    expect(follows).toEqual([[0, null]]);
  });

  it('takes only an arrow while the moved path is found again, and nothing for another kind', () => {
    const { hook, applied } = setup({ path: null, following: 'path', pending: true });
    expect(hook.result.current.onKey('ArrowUp', false)).toBe(true);
    expect(hook.result.current.onKey('Delete', false)).toBe(false);
    expect(applied).toEqual([]);
    const other = setup({ path: null, following: 'image' });
    expect(other.hook.result.current.onKey('ArrowUp', false)).toBe(false);
  });

  it('asks for nothing while a cut is being read', () => {
    const { hook, applied } = setup({ clipping: true });
    expect(hook.result.current.busy).toBe(true);
    act(() => {
      hook.result.current.remove();
      hook.result.current.resize({ left: 0, top: 0, width: 10, height: 10 });
    });
    expect(applied).toEqual([]);
  });

  it('refuses the step when the page at the path’s position changed before its turn', async () => {
    let error: unknown = null;
    const moved = { info: { ...info, pageIds: [10] }, annotations: [] };
    const apply = async (build: (s: Session) => Edit | null) => {
      try {
        build(moved);
      } catch (thrown) {
        error = thrown;
      }
    };
    const content = { active: true, selected: { pageIndex: 0, key: '9/0/0/0', item: shape }, following: false, followingKind: null, followPath: () => {} } as unknown as ContentMode;
    const { result } = renderHook(() => usePathEditing({ info, content, queue: { pending: false, apply, notify: () => {} } }));
    act(() => result.current.remove());
    await waitFor(() => expect(error).toBeInstanceOf(Error));
    expect(String(error)).toMatch(/page changed/i);
  });
});

describe('PathOverlay', () => {
  const handles = () => screen.queryAllByRole('button', { name: /^Resize from/ });

  it('gives a shape eight handles and a rule the two at its ends, none while busy', () => {
    const view = render(<PathOverlay path={shape} scale={1} busy={false} onResize={() => {}} onRemove={() => {}} />);
    expect(handles()).toHaveLength(8);
    view.rerender(<PathOverlay path={rule} scale={1} busy={false} onResize={() => {}} onRemove={() => {}} />);
    expect(handles().map((handle) => handle.getAttribute('aria-label'))).toEqual(['Resize from the right side', 'Resize from the left side']);
    view.rerender(<PathOverlay path={rule} scale={1} busy onResize={() => {}} onRemove={() => {}} />);
    expect(handles()).toHaveLength(0);
    expect(screen.getByRole<HTMLButtonElement>('button', { name: 'Delete shape' }).disabled).toBe(true);
  });

  it('reports the box a corner is let go at, free unless Shift keeps the proportions', () => {
    const onResize = vi.fn();
    render(<PathOverlay path={shape} scale={2} busy={false} onResize={onResize} onRemove={() => {}} />);
    const corner = screen.getByRole('button', { name: 'Resize from the bottom-right corner' });
    fireEvent.pointerDown(corner, { button: 0, isPrimary: true, pointerId: 1, clientX: 600, clientY: 600 });
    fireEvent.pointerMove(corner, { pointerId: 1, clientX: 700, clientY: 620 });
    // The toolbar steps aside while a handle is dragged, the box it would give drawn instead.
    expect(screen.queryByRole('toolbar', { name: 'Shape' })).toBeNull();
    fireEvent.pointerUp(corner, { pointerId: 1, clientX: 700, clientY: 620 });
    expect(onResize).toHaveBeenLastCalledWith({ left: 100, top: 200, width: 250, height: 110 });

    fireEvent.pointerDown(corner, { button: 0, isPrimary: true, pointerId: 2, clientX: 600, clientY: 600 });
    fireEvent.pointerMove(corner, { pointerId: 2, clientX: 700, clientY: 620, shiftKey: true });
    fireEvent.pointerUp(corner, { pointerId: 2, clientX: 700, clientY: 620, shiftKey: true });
    expect(onResize).toHaveBeenLastCalledWith({ left: 100, top: 200, width: 250, height: 125 });
  });

  it('lengthens a rule from its end and keeps it flat', () => {
    const onResize = vi.fn();
    render(<PathOverlay path={rule} scale={1} busy={false} onResize={onResize} onRemove={() => {}} />);
    const end = screen.getByRole('button', { name: 'Resize from the right side' });
    fireEvent.pointerDown(end, { button: 0, isPrimary: true, pointerId: 1, clientX: 272, clientY: 392 });
    fireEvent.pointerMove(end, { pointerId: 1, clientX: 322, clientY: 420 });
    fireEvent.pointerUp(end, { pointerId: 1, clientX: 322, clientY: 420 });
    expect(onResize).toHaveBeenCalledWith({ left: 72, top: 392, width: 250, height: 0 });
  });

  it('deletes from its toolbar', () => {
    const onRemove = vi.fn();
    render(<PathOverlay path={shape} scale={1} busy={false} onResize={() => {}} onRemove={onRemove} />);
    fireEvent.click(screen.getByRole('button', { name: 'Delete shape' }));
    expect(onRemove).toHaveBeenCalled();
  });
});
