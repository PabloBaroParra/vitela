import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { EngineError, errorMessage, INVALID_IMAGE } from '@vitela/core';
import type { AnnotationEdit, DocumentHandle, PagePoint } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { MAX_STAMP_BYTES } from './stamps';
import type { AddOutcome, PendingEdit } from './useAnnotationEdits';
import type { DrawingEdits } from './useDrawing';
import { NOT_A_STAMP_IMAGE, STAMP_NOT_ALLOWED, STAMP_NOT_READ, STAMP_TOO_LARGE, useStamps } from './useStamps';
import { NOT_DRAWN } from './useDrawing';

const PNG = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
const BOX = { left: 100, top: 200, width: 144, height: 72 };
const png = (name = 'logo.png') => new File([new Uint8Array(PNG)], name, { type: 'image/png' });

/** A PNG whose bytes arrive when the test says so. */
function slowPng(name: string) {
  const file = png(name);
  let finish: () => void = () => {};
  const ready = new Promise<void>((resolve) => (finish = resolve));
  const bytes = file.arrayBuffer();
  vi.spyOn(file, 'arrayBuffer').mockImplementation(() => ready.then(() => bytes));
  return { file, finish };
}

interface Options {
  readonly outcome?: AddOutcome;
  readonly allowed?: boolean;
  /** Another step is running: `add` refuses before building anything, as the real queue does. */
  readonly busy?: boolean;
  readonly placement?: () => Promise<typeof BOX>;
}

function stamps(options: Options = {}) {
  const { outcome = { status: 'added' }, allowed = true, busy = false } = options;
  const stampPlacement = vi.fn(
    (_handle: DocumentHandle, _page: number, _image: Uint8Array, _anchor: PagePoint) => options.placement?.() ?? Promise.resolve(BOX),
  );
  const built: AnnotationEdit[] = [];
  // Like `useAnnotationEdits.add`: a deferred edit is built inside the step.
  const add = vi.fn(
    async (edit: PendingEdit, _page: number, _key: string, _shown?: readonly string[], onAdded?: (id: number) => void): Promise<AddOutcome> => {
      if (busy) return { status: 'refused' };
      try {
        built.push(typeof edit === 'function' ? await edit() : edit);
      } catch (error) {
        return { status: 'failed', kind: null, message: errorMessage(error) };
      }
      if (outcome.status === 'added') onAdded?.(17);
      return outcome;
    },
  );
  const notify = vi.fn();
  const keep = vi.fn();
  const edits: DrawingEdits = { allowed, add, notify };
  const view = renderHook(() => useStamps(fakeEngine({ stampPlacement }), 3, edits, keep));
  return { ...view, stampPlacement, add, built, notify, keep };
}

describe('useStamps', () => {
  it('picks an image to stamp with, keeping its bytes and a drawable URL', async () => {
    const { result } = stamps();
    expect(result.current.image).toBeNull();
    let picked = false;
    await act(async () => {
      picked = await result.current.pick(png());
    });
    expect(picked).toBe(true);
    expect(result.current.image?.name).toBe('logo.png');
    expect([...result.current.image!.bytes]).toEqual(PNG);
    expect(result.current.image?.href).toMatch(/^data:image\/png;base64,/);
  });

  it('refuses a file that is not a PNG or JPEG, before the core sees it', async () => {
    const { result, notify, stampPlacement } = stamps();
    let picked = true;
    await act(async () => {
      picked = await result.current.pick(new File(['%PDF-1.7'], 'fake.png', { type: 'image/png' }));
    });
    expect(picked).toBe(false);
    expect(result.current.image).toBeNull();
    expect(notify).toHaveBeenCalledWith(NOT_A_STAMP_IMAGE);
    expect(stampPlacement).not.toHaveBeenCalled();
  });

  it('says the file could not be read, not that it is the wrong type, when reading it fails', async () => {
    const { result, notify } = stamps();
    const file = png();
    vi.spyOn(file, 'arrayBuffer').mockRejectedValue(new DOMException('gone', 'NotReadableError'));
    let picked = true;
    await act(async () => {
      picked = await result.current.pick(file);
    });
    expect(picked).toBe(false);
    expect(notify).toHaveBeenCalledWith(STAMP_NOT_READ);
    expect(notify).not.toHaveBeenCalledWith(NOT_A_STAMP_IMAGE);
  });

  it('refuses an image over the size limit without reading it', async () => {
    const { result, notify, add } = stamps();
    const file = png('huge.png');
    Object.defineProperty(file, 'size', { value: MAX_STAMP_BYTES + 1 });
    const read = vi.spyOn(file, 'arrayBuffer');
    let picked = true;
    await act(async () => {
      picked = await result.current.pick(file);
    });
    expect(picked).toBe(false);
    expect(read).not.toHaveBeenCalled();
    expect(notify).toHaveBeenCalledWith(STAMP_TOO_LARGE);

    act(() => result.current.drop(0, '0/0', { x: 1, y: 1 }, file));
    await waitFor(() => expect(notify).toHaveBeenCalledTimes(2));
    expect(read).not.toHaveBeenCalled();
    expect(add).not.toHaveBeenCalled();
  });

  it('keeps the latest pick even when an earlier one finishes reading after it', async () => {
    const { result } = stamps();
    const first = slowPng('first.png');
    let firstPicked: Promise<boolean> = Promise.resolve(true);
    let secondPicked = false;
    await act(async () => {
      firstPicked = result.current.pick(first.file);
      secondPicked = await result.current.pick(png('second.png'));
    });
    expect(secondPicked).toBe(true);
    expect(result.current.image?.name).toBe('second.png');

    await act(async () => {
      first.finish();
      expect(await firstPicked).toBe(false);
    });
    expect(result.current.image?.name).toBe('second.png');
  });

  it('places the armed image inside its queued step, adds it as one edit and keeps its image by id', async () => {
    const { result, stampPlacement, add, built, keep } = stamps();
    await act(async () => {
      await result.current.pick(png());
    });
    act(() => result.current.place(1, '4/0', { x: 50, y: 60 }));
    await waitFor(() => expect(keep).toHaveBeenCalledWith(17, result.current.image!.href));

    expect(add).toHaveBeenCalledTimes(1);
    const [edit, page, key] = add.mock.calls[0]!;
    // Built by the queue, not before it: the page is busy while the core places it.
    expect(typeof edit).toBe('function');
    expect([page, key]).toEqual([1, '4/0']);
    expect(stampPlacement).toHaveBeenCalledWith(3, 1, result.current.image!.bytes, { x: 50, y: 60 });
    expect(built).toEqual([{ kind: 'addStamp', page: 1, rect: BOX, image: result.current.image!.bytes }]);
    // The tool stays armed with the same image for the next click.
    expect(result.current.image).not.toBeNull();
  });

  it('asks the core nothing for a stamp refused because another step is running', async () => {
    const { result, stampPlacement, notify, keep } = stamps({ busy: true });
    await act(async () => {
      await result.current.pick(png());
    });
    act(() => result.current.place(0, '0/0', { x: 1, y: 1 }));
    await waitFor(() => expect(notify).toHaveBeenCalledWith(NOT_DRAWN));
    expect(stampPlacement).not.toHaveBeenCalled();
    expect(keep).not.toHaveBeenCalled();
  });

  it('places nothing with no image picked', async () => {
    const { result, add } = stamps();
    act(() => result.current.place(0, '0/0', { x: 1, y: 1 }));
    await act(async () => {});
    expect(add).not.toHaveBeenCalled();
  });

  it('stamps a dropped image at the drop point without arming it', async () => {
    const { result, built, keep } = stamps();
    act(() => result.current.drop(0, '0/0', { x: 10, y: 20 }, png('drop.png')));
    await waitFor(() => expect(keep).toHaveBeenCalledWith(17, expect.stringMatching(/^data:image\/png;base64,/)));
    expect(built[0]).toMatchObject({ kind: 'addStamp', page: 0, rect: BOX });
    expect(result.current.image).toBeNull();
  });

  it('leaves a placement the core cannot decode to the viewer, which shows it, and keeps nothing', async () => {
    const { result, add, notify, keep } = stamps({
      placement: () => Promise.reject(new EngineError(INVALID_IMAGE, 'The image could not be decoded')),
    });
    act(() => result.current.drop(0, '0/0', { x: 10, y: 20 }, png()));
    await waitFor(() => expect(add).toHaveBeenCalledTimes(1));
    await expect(add.mock.results[0]!.value).resolves.toEqual({
      status: 'failed',
      kind: null,
      message: 'The image could not be decoded',
    });
    expect(notify).not.toHaveBeenCalled();
    expect(keep).not.toHaveBeenCalled();
  });

  it('says why when annotating is forbidden, and asks the core nothing', async () => {
    const { result, stampPlacement, add, notify } = stamps({ allowed: false });
    act(() => result.current.drop(0, '0/0', { x: 10, y: 20 }, png()));
    await waitFor(() => expect(notify).toHaveBeenCalledWith(STAMP_NOT_ALLOWED));
    expect(stampPlacement).not.toHaveBeenCalled();
    expect(add).not.toHaveBeenCalled();
  });

  it('leaves a failed add to the viewer, which already shows it', async () => {
    const { result, add, notify, keep } = stamps({ outcome: { status: 'failed', kind: null, message: 'boom' } });
    act(() => result.current.drop(0, '0/0', { x: 10, y: 20 }, png()));
    await waitFor(() => expect(add).toHaveBeenCalledTimes(1));
    await add.mock.results[0]!.value;
    expect(notify).not.toHaveBeenCalled();
    expect(keep).not.toHaveBeenCalled();
  });
});
