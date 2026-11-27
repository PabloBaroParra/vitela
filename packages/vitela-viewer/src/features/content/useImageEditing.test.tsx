import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ContentImage, Edit, Engine } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { Session } from '../annotations/useAnnotationEdits';
import { MAX_STAMP_BYTES } from '../annotations/stamps';
import { ImageOverlay } from './ImageOverlay';
import type { ContentMode } from './useContentMode';
import { INSERT_IMAGE_PROMPT, useImageEditing } from './useImageEditing';

afterEach(cleanup);

const photo: ContentImage = { kind: 'image', id: 3, pageIndex: 0, rect: { left: 100, top: 200, width: 200, height: 100 }, inline: false };
const info = { pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }], pageIds: [9] };
const session: Session = { info, annotations: [] };
const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3]);

const pngFile = (bytes: Uint8Array = PNG, name = 'logo.png') => new File([bytes as Uint8Array<ArrayBuffer>], name, { type: 'image/png' });

function setup({
  image = photo as ContentImage | null,
  pending = false,
  following = null as 'run' | 'image' | null,
  refusal = null as string | null,
  engine: overrides = {} as Partial<Engine>,
} = {}) {
  const applied: (Edit | null)[] = [];
  const follows: unknown[][] = [];
  const notices: string[] = [];
  const checks: unknown[][] = [];
  const engine = fakeEngine({
    checkImageReplacement: async (...args) => {
      checks.push(args);
      if (refusal) throw Object.assign(new Error(refusal), { name: 'UnsupportedOperation' });
    },
    stampPlacement: async (_document, _page, _bytes, anchor) => ({ left: anchor.x, top: anchor.y, width: 144, height: 72 }),
    ...overrides,
  });
  const content = (selected: ContentImage | null, key: string) =>
    ({
      active: true,
      selected: selected && { pageIndex: selected.pageIndex, key, item: selected },
      keyAt: () => key,
      following: following !== null,
      followingKind: following,
      followImage: (...args: unknown[]) => void follows.push(args),
    }) as unknown as ContentMode;
  const queue = {
    pending,
    apply: async (build: (session: Session) => Edit | null) => void applied.push(build(session)),
    notify: (message: string) => void notices.push(message),
  };
  const focusPages = vi.fn();
  const hook = renderHook(
    ({ running, selected, key = '9/0/0/0' }: { running: boolean; selected: ContentImage | null; key?: string }) =>
      useImageEditing({ engine, handle: 1, info, content: content(selected, key), queue: { ...queue, pending: running }, focusPages }),
    { initialProps: { running: pending, selected: image } as { running: boolean; selected: ContentImage | null; key?: string } },
  );
  return { hook, applied, follows, notices, checks, focusPages };
}

describe('useImageEditing', () => {
  it('moves the selected image by an arrow and follows it to its new place', () => {
    const { hook, applied, follows } = setup();
    let used = false;
    act(() => void (used = hook.result.current.onKey('ArrowDown', true)));
    expect(used).toBe(true);
    expect(applied).toEqual([{ kind: 'moveImage', page: 0, imageId: 3, dx: 0, dy: 10 }]);
    expect(follows).toEqual([[0, { ...photo.rect, top: 210 }]]);
    expect(hook.result.current.onKey('Enter', false)).toBe(false);
  });

  it('adds up the arrows pressed while a step runs into one move once it is done', () => {
    const { hook, applied } = setup({ pending: true });
    act(() => {
      hook.result.current.move(1, 0);
      hook.result.current.move(1, 0);
    });
    expect(applied).toEqual([]);
    hook.rerender({ running: false, selected: photo });
    expect(applied).toEqual([{ kind: 'moveImage', page: 0, imageId: 3, dx: 2, dy: 0 }]);
  });

  it('resizes to a box and follows it there; the box it has asks for nothing', () => {
    const { hook, applied, follows } = setup();
    const rect = { left: 100, top: 200, width: 300, height: 150 };
    act(() => hook.result.current.resize(rect));
    expect(applied).toEqual([{ kind: 'resizeImage', page: 0, imageId: 3, rect }]);
    expect(follows).toEqual([[0, rect]]);
    act(() => hook.result.current.resize(photo.rect));
    expect(applied).toHaveLength(1);
  });

  it('deletes on Delete or Backspace and follows nothing', () => {
    const { hook, applied, follows } = setup();
    act(() => void hook.result.current.onKey('Delete', false));
    expect(applied).toEqual([{ kind: 'removeImage', page: 0, imageId: 3 }]);
    expect(follows).toEqual([[0, null]]);
  });

  it('refuses the step when the page at the image’s position changed before its turn', async () => {
    const { hook } = setup();
    let error: unknown = null;
    const moved = { info: { ...info, pageIds: [10] }, annotations: [] };
    const apply = async (build: (s: Session) => Edit | null) => {
      try {
        build(moved);
      } catch (thrown) {
        error = thrown;
      }
    };
    const engine = fakeEngine({ checkImageReplacement: async () => {} });
    const content = { active: true, selected: { pageIndex: 0, key: '9/0/0/0', item: photo }, following: false, followingKind: null, followImage: () => {} } as unknown as ContentMode;
    const other = renderHook(() =>
      useImageEditing({ engine, handle: 1, info, content, queue: { pending: false, apply, notify: () => {} }, focusPages: () => {} }),
    );
    act(() => other.result.current.remove());
    await waitFor(() => expect(error).toBeInstanceOf(Error));
    expect(String(error)).toMatch(/page changed/i);
    hook.unmount();
  });

  it('asks the core whether the image can be replaced before any file is picked, and says why not', async () => {
    const { hook, checks, notices } = setup({ refusal: 'save before replacing an image with a pending edit' });
    await waitFor(() => expect(hook.result.current.replaceRefusal).toBe('Save before replacing an image with a pending edit.'));
    expect(checks).toEqual([[1, 0, 3]]);
    const opened = vi.fn();
    act(() => hook.result.current.replace(opened));
    expect(opened).not.toHaveBeenCalled();
    expect(notices).toEqual(['Save before replacing an image with a pending edit.']);
  });

  it('replaces the image with a picked PNG once the core said it could, and follows it in place', async () => {
    const { hook, applied, follows } = setup();
    await waitFor(() => expect(hook.result.current.replaceRefusal).toBeNull());
    await waitFor(() => expect(hook.result.current.replaceable).toBe(true));
    const opened = vi.fn();
    act(() => hook.result.current.replace(opened));
    expect(opened).toHaveBeenCalled();
    await act(async () => hook.result.current.picked(pngFile()));
    expect(applied).toEqual([{ kind: 'replaceImage', page: 0, imageId: 3, bytes: PNG }]);
    expect(follows).toEqual([[0, photo.rect]]);
  });

  it('refuses a picked file that is not a PNG or JPEG, or is too large, before the core sees it', async () => {
    const { hook, applied, notices } = setup();
    await waitFor(() => expect(hook.result.current.replaceable).toBe(true));
    act(() => hook.result.current.replace(() => {}));
    await act(async () => hook.result.current.picked(pngFile(new Uint8Array([1, 2, 3]), 'notes.png')));
    const big = pngFile();
    Object.defineProperty(big, 'size', { value: MAX_STAMP_BYTES + 1 });
    act(() => hook.result.current.replace(() => {}));
    await act(async () => hook.result.current.picked(big));
    expect(applied).toEqual([]);
    expect(notices).toEqual(['Only PNG and JPEG images can be added to a page.', 'Images larger than 20 MB cannot be added to a page.']);
  });

  it('arms an insert with a picked image, places it where the page is clicked, and selects it there', async () => {
    const { hook, applied, follows } = setup({ image: null });
    expect(hook.result.current.armed).toBe(false);
    const opened = vi.fn();
    act(() => hook.result.current.insert(opened));
    expect(opened).toHaveBeenCalled();
    await act(async () => hook.result.current.picked(pngFile()));
    expect(hook.result.current.armed).toBe(true);
    expect(hook.result.current.prompt).toBe(INSERT_IMAGE_PROMPT);
    let placed = false;
    await act(async () => void (placed = hook.result.current.place(0, { x: 50, y: 60 })));
    expect(placed).toBe(true);
    await waitFor(() => expect(applied).toEqual([{ kind: 'insertImage', page: 0, at: { x: 50, y: 60 }, bytes: PNG }]));
    expect(follows).toEqual([[0, { left: 50, top: 60, width: 144, height: 72 }]]);
    expect(hook.result.current.armed).toBe(false);
    // A click once disarmed is a click, not a placement.
    expect(hook.result.current.place(0, { x: 1, y: 1 })).toBe(false);
  });

  it('gives the keyboard to the pages once an insert is armed, so Escape there disarms it', async () => {
    const { hook, focusPages } = setup({ image: null });
    act(() => hook.result.current.insert(() => {}));
    expect(focusPages).not.toHaveBeenCalled();
    await act(async () => hook.result.current.picked(pngFile()));
    expect(hook.result.current.armed).toBe(true);
    expect(focusPages).toHaveBeenCalled();
  });

  it('places an armed insert only once no step runs, and keeps it armed meanwhile', async () => {
    const placements: unknown[] = [];
    const { hook, applied } = setup({
      image: null,
      engine: {
        stampPlacement: async (_document, _page, _bytes, anchor) => {
          placements.push(anchor);
          return { left: anchor.x, top: anchor.y, width: 144, height: 72 };
        },
      },
    });
    act(() => hook.result.current.insert(() => {}));
    await act(async () => hook.result.current.picked(pngFile()));
    hook.rerender({ running: true, selected: null });
    let placed = false;
    await act(async () => void (placed = hook.result.current.place(0, { x: 50, y: 60 })));
    // The press is the insert's (it selects nothing), but waits for the step.
    expect(placed).toBe(true);
    expect(placements).toEqual([]);
    expect(applied).toEqual([]);
    expect(hook.result.current.armed).toBe(true);
    await act(async () => hook.rerender({ running: false, selected: null }));
    await waitFor(() => expect(applied).toEqual([{ kind: 'insertImage', page: 0, at: { x: 50, y: 60 }, bytes: PNG }]));
    expect(hook.result.current.armed).toBe(false);
  });

  it('waits for a step started by the same press (a retype it committed) before placing', async () => {
    const placements: unknown[] = [];
    const { hook, applied } = setup({
      image: null,
      engine: {
        stampPlacement: async (_document, _page, _bytes, anchor) => {
          placements.push(anchor);
          return { left: anchor.x, top: anchor.y, width: 144, height: 72 };
        },
      },
    });
    act(() => hook.result.current.insert(() => {}));
    await act(async () => hook.result.current.picked(pngFile()));
    // The press committed a retype first: the step starts in the same turn.
    act(() => {
      hook.result.current.place(0, { x: 50, y: 60 });
      hook.rerender({ running: true, selected: null });
    });
    expect(placements).toEqual([]);
    expect(applied).toEqual([]);
    await act(async () => hook.rerender({ running: false, selected: null }));
    await waitFor(() => expect(applied).toHaveLength(1));
  });

  it('drops a queued placement on Escape', async () => {
    const { hook, applied } = setup({ image: null });
    act(() => hook.result.current.insert(() => {}));
    await act(async () => hook.result.current.picked(pngFile()));
    hook.rerender({ running: true, selected: null });
    act(() => void hook.result.current.place(0, { x: 50, y: 60 }));
    act(() => void hook.result.current.onKey('Escape', false));
    await act(async () => hook.rerender({ running: false, selected: null }));
    expect(applied).toEqual([]);
    expect(hook.result.current.armed).toBe(false);
  });

  it('refuses a replacement whose page was read anew while its file was read', async () => {
    const { hook, applied, notices } = setup();
    await waitFor(() => expect(hook.result.current.replaceable).toBe(true));
    act(() => hook.result.current.replace(() => {}));
    const reading = hook.result.current.picked(pngFile());
    // An edit elsewhere on the page landed meanwhile: the ids are another read's.
    hook.rerender({ running: false, selected: photo, key: '9/0/1/0' });
    await act(async () => reading);
    expect(applied).toEqual([]);
    expect(notices).toEqual(['The page changed before the image could be edited. Nothing was changed.']);
  });

  it('refuses a replacement while a step runs once its file is read', async () => {
    const { hook, applied, notices } = setup();
    await waitFor(() => expect(hook.result.current.replaceable).toBe(true));
    act(() => hook.result.current.replace(() => {}));
    const reading = hook.result.current.picked(pngFile());
    hook.rerender({ running: true, selected: photo });
    await act(async () => reading);
    expect(applied).toEqual([]);
    expect(notices).toHaveLength(1);
  });

  it('disarms on Escape, and says nothing was added', async () => {
    const { hook, applied } = setup({ image: null });
    act(() => hook.result.current.insert(() => {}));
    await act(async () => hook.result.current.picked(pngFile()));
    let used = false;
    act(() => void (used = hook.result.current.onKey('Escape', false)));
    expect(used).toBe(true);
    expect(hook.result.current.armed).toBe(false);
    expect(hook.result.current.place(0, { x: 1, y: 1 })).toBe(false);
    expect(applied).toEqual([]);
  });

  it('says why an image the core cannot place is not added', async () => {
    const { hook, applied, notices } = setup({
      image: null,
      engine: { stampPlacement: async () => Promise.reject(Object.assign(new Error('invalid image: bad PNG'), { name: 'InvalidImage' })) },
    });
    act(() => hook.result.current.insert(() => {}));
    await act(async () => hook.result.current.picked(pngFile()));
    await act(async () => void hook.result.current.place(0, { x: 1, y: 1 }));
    await waitFor(() => expect(notices).toEqual(['Invalid image: bad PNG.']));
    expect(applied).toEqual([]);
  });

  it('asks for nothing while a step runs or an edited item is found again', () => {
    const { hook, applied } = setup({ following: 'run' });
    act(() => hook.result.current.remove());
    act(() => hook.result.current.resize({ left: 0, top: 0, width: 10, height: 10 }));
    expect(applied).toEqual([]);
  });
});

describe('ImageOverlay', () => {
  const base = {
    image: photo,
    scale: 2,
    busy: false,
    replaceable: true,
    onResize: () => {},
    onReplace: () => {},
    onRemove: () => {},
  };

  it('draws eight resize handles and a toolbar with Replace and Delete', () => {
    const onReplace = vi.fn();
    const onRemove = vi.fn();
    render(<ImageOverlay {...base} onReplace={onReplace} onRemove={onRemove} />);
    expect(screen.getAllByRole('button', { name: /^Resize/ })).toHaveLength(8);
    fireEvent.click(screen.getByRole('button', { name: 'Replace image' }));
    fireEvent.click(screen.getByRole('button', { name: 'Delete image' }));
    expect(onReplace).toHaveBeenCalled();
    expect(onRemove).toHaveBeenCalled();
  });

  it('resizes from a corner keeping the proportions, and freely with Shift', () => {
    const onResize = vi.fn();
    render(<ImageOverlay {...base} onResize={onResize} />);
    const corner = screen.getByRole('button', { name: 'Resize from the bottom-right corner' });
    fireEvent.pointerDown(corner, { pointerId: 1, button: 0, isPrimary: true, clientX: 600, clientY: 600 });
    fireEvent.pointerMove(corner, { pointerId: 1, clientX: 800, clientY: 610 });
    expect(document.querySelector('.image-edit__preview')).not.toBeNull();
    fireEvent.pointerUp(corner, { pointerId: 1, clientX: 800, clientY: 610 });
    // 200 px at scale 2 = 100 pt wider; the height follows.
    expect(onResize).toHaveBeenLastCalledWith({ left: 100, top: 200, width: 300, height: 150 });

    fireEvent.pointerDown(corner, { pointerId: 2, button: 0, isPrimary: true, clientX: 600, clientY: 600 });
    fireEvent.pointerUp(corner, { pointerId: 2, clientX: 800, clientY: 610, shiftKey: true });
    expect(onResize).toHaveBeenLastCalledWith({ left: 100, top: 200, width: 300, height: 105 });
  });

  it('asks for no resize for a press on a handle that does not travel', () => {
    const onResize = vi.fn();
    render(<ImageOverlay {...base} onResize={onResize} />);
    const edge = screen.getByRole('button', { name: 'Resize from the right side' });
    fireEvent.pointerDown(edge, { pointerId: 1, button: 0, isPrimary: true, clientX: 600, clientY: 500 });
    fireEvent.pointerUp(edge, { pointerId: 1, clientX: 601, clientY: 500 });
    expect(onResize).not.toHaveBeenCalled();
  });

  it('offers nothing while a step runs, and no Replace the core refused', () => {
    const { rerender } = render(<ImageOverlay {...base} busy />);
    expect((screen.getByRole('button', { name: 'Delete image' }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryAllByRole('button', { name: /^Resize/ })).toHaveLength(0);
    rerender(<ImageOverlay {...base} replaceable={false} />);
    expect(screen.getByRole('button', { name: 'Replace image' }).getAttribute('aria-disabled')).toBe('true');
  });
});
