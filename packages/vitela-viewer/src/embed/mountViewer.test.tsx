import { act, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DocumentInfo, RenderedPage } from '@vitela/core';
import { fakeEngine } from '../test/fakeEngine';
import { mountViewer, type MountedViewer } from './mountViewer';

const info: DocumentInfo = {
  pageCount: 1,
  pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }],
  pageIds: [0],
  textExtractionAllowed: true,
  documentAssemblyAllowed: true,
  blocks: [{ source: { kind: 'base' }, part: null, start: 0, count: 1 }],
  importRefusal: null,
  annotationEditingAllowed: true,
};
const page: RenderedPage = { width: 1, height: 1, rgba: new Uint8ClampedArray(4) };

let element: HTMLElement;
beforeEach(() => {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
    putImageData: () => {},
  } as unknown as CanvasRenderingContext2D);
  vi.stubGlobal('ImageData', class {});
  element = document.body.appendChild(document.createElement('div'));
});
afterEach(() => {
  element.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('mountViewer', () => {
  it('mounts the editor into the element, opens and saves through the handle, and unmounts on destroy', async () => {
    const closed: number[] = [];
    const engine = fakeEngine({
      open: async () => ({ handle: 4, info }),
      renderPage: async () => page,
      saveToBytes: async () => new Uint8Array([9]),
      close: async (handle) => void closed.push(handle),
    });
    const onSave = vi.fn();
    let viewer!: MountedViewer;
    act(() => {
      viewer = mountViewer(element, { engine, onSave });
    });
    expect(element.querySelector('.vitela')).not.toBeNull();

    act(() => viewer.open(new Uint8Array([37]), 'mounted.pdf'));
    expect(await screen.findByText('mounted.pdf — 1 page')).toBeTruthy();
    act(() => viewer.save());
    await waitFor(() => expect(onSave).toHaveBeenCalledExactlyOnceWith(new Uint8Array([9]), 'mounted.pdf'));

    act(() => viewer.destroy());
    expect(element.childElementCount).toBe(0);
    // The host's engine outlives the editor; the editor's document does not.
    expect(closed).toEqual([4]);
    // Calls after destroy do nothing, and destroying again is harmless.
    expect(() => {
      viewer.open(new Uint8Array([37]));
      viewer.save();
      viewer.destroy();
    }).not.toThrow();
  });

  it('opens its source as soon as the engine is ready', async () => {
    let viewer!: MountedViewer;
    const engine = fakeEngine({ open: async () => ({ handle: 1, info }), renderPage: async () => page });
    act(() => {
      viewer = mountViewer(element, { engine, source: new Uint8Array([37]), name: 'initial.pdf', filePicker: false });
    });
    expect(await screen.findByText('initial.pdf — 1 page')).toBeTruthy();
    expect(screen.queryByLabelText('Open PDF')).toBeNull();
    act(() => viewer.destroy());
  });
});
