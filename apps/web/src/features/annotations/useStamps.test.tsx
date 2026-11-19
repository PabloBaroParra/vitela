import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { EngineError, INVALID_IMAGE } from '@vitela/core';
import type { AnnotationEdit, DocumentHandle, PagePoint } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { AddOutcome } from './useAnnotationEdits';
import type { DrawingEdits } from './useDrawing';
import { NOT_A_STAMP_IMAGE, STAMP_NOT_ALLOWED, useStamps } from './useStamps';
import { NOT_DRAWN } from './useDrawing';

const PNG = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
const BOX = { left: 100, top: 200, width: 144, height: 72 };
const png = (name = 'logo.png') => new File([new Uint8Array(PNG)], name, { type: 'image/png' });

function stamps(options: { outcome?: AddOutcome; allowed?: boolean; placement?: () => Promise<typeof BOX> } = {}) {
  const { outcome = { status: 'added' }, allowed = true } = options;
  const stampPlacement = vi.fn(
    (_handle: DocumentHandle, _page: number, _image: Uint8Array, _anchor: PagePoint) => options.placement?.() ?? Promise.resolve(BOX),
  );
  const add = vi.fn(
    async (_edit: AnnotationEdit, _page: number, _key: string, _shown?: readonly string[], onAdded?: (id: number) => void) => {
      if (outcome.status === 'added') onAdded?.(17);
      return outcome;
    },
  );
  const notify = vi.fn();
  const keep = vi.fn();
  const edits: DrawingEdits = { allowed, add, notify };
  const view = renderHook(() => useStamps(fakeEngine({ stampPlacement }), 3, edits, keep));
  return { ...view, stampPlacement, add, notify, keep };
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

  it('places the armed image where the core says, adds it as one edit and keeps its image by id', async () => {
    const { result, stampPlacement, add, keep } = stamps();
    await act(async () => {
      await result.current.pick(png());
    });
    await act(async () => {
      await result.current.place(1, '4/0', { x: 50, y: 60 });
    });

    expect(stampPlacement).toHaveBeenCalledWith(3, 1, result.current.image!.bytes, { x: 50, y: 60 });
    expect(add).toHaveBeenCalledTimes(1);
    const [edit, page, key] = add.mock.calls[0]!;
    expect(edit).toEqual({ kind: 'addStamp', page: 1, rect: BOX, image: result.current.image!.bytes });
    expect([page, key]).toEqual([1, '4/0']);
    expect(keep).toHaveBeenCalledWith(17, result.current.image!.href);
    // The tool stays armed with the same image for the next click.
    expect(result.current.image).not.toBeNull();
  });

  it('places nothing with no image picked', async () => {
    const { result, stampPlacement } = stamps();
    await act(async () => {
      await result.current.place(0, '0/0', { x: 1, y: 1 });
    });
    expect(stampPlacement).not.toHaveBeenCalled();
  });

  it('stamps a dropped image at the drop point without arming it', async () => {
    const { result, add, keep } = stamps();
    await act(async () => {
      await result.current.drop(0, '0/0', { x: 10, y: 20 }, png('drop.png'));
    });
    expect(add.mock.calls[0]![0]).toMatchObject({ kind: 'addStamp', page: 0, rect: BOX });
    expect(keep).toHaveBeenCalledWith(17, expect.stringMatching(/^data:image\/png;base64,/));
    expect(result.current.image).toBeNull();
  });

  it('says why when the core cannot decode the image, and adds nothing', async () => {
    const { result, add, notify } = stamps({
      placement: () => Promise.reject(new EngineError(INVALID_IMAGE, 'The image could not be decoded')),
    });
    await act(async () => {
      await result.current.drop(0, '0/0', { x: 10, y: 20 }, png());
    });
    expect(add).not.toHaveBeenCalled();
    expect(notify).toHaveBeenCalledWith('The image could not be decoded');
  });

  it('says why when annotating is forbidden or another step was running, and keeps nothing', async () => {
    const forbidden = stamps({ allowed: false });
    await act(async () => {
      await forbidden.result.current.drop(0, '0/0', { x: 10, y: 20 }, png());
    });
    expect(forbidden.stampPlacement).not.toHaveBeenCalled();
    expect(forbidden.notify).toHaveBeenCalledWith(STAMP_NOT_ALLOWED);

    const busy = stamps({ outcome: { status: 'refused' } });
    await act(async () => {
      await busy.result.current.drop(0, '0/0', { x: 10, y: 20 }, png());
    });
    expect(busy.notify).toHaveBeenCalledWith(NOT_DRAWN);
    expect(busy.keep).not.toHaveBeenCalled();
  });

  it('leaves a failed add to the viewer, which already shows it', async () => {
    const { result, notify, keep } = stamps({ outcome: { status: 'failed', kind: null, message: 'boom' } });
    await act(async () => {
      await result.current.drop(0, '0/0', { x: 10, y: 20 }, png());
    });
    expect(notify).not.toHaveBeenCalled();
    expect(keep).not.toHaveBeenCalled();
  });
});
