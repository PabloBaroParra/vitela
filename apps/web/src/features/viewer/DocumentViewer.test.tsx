import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Engine, PageSize, RenderedPage } from '../../core/types';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import { DocumentViewer } from './DocumentViewer';

const VIEWPORT = { width: 800, height: 600 };
const page: RenderedPage = { width: 1, height: 1, rgba: new Uint8ClampedArray(4) };
const pages: PageSize[] = Array(40).fill({ widthPt: 612, heightPt: 792, rotationDegrees: 0 });
const doc: OpenDocument = {
  name: 'long.pdf',
  info: { pageCount: pages.length, pages, textExtractionAllowed: true },
  handle: 1,
  generation: 1,
};

const engine: Engine = fakeEngine({ renderPage: async () => page });

/** jsdom lays nothing out: give every element the viewer's viewport and real scroll offsets. */
function fakeLayout() {
  const offsets = new WeakMap<Element, { top: number; left: number }>();
  const offset = (element: Element) => offsets.get(element) ?? { top: 0, left: 0 };
  const restore = [
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(VIEWPORT.width),
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(VIEWPORT.height),
    vi.spyOn(Element.prototype, 'scrollTop', 'get').mockImplementation(function (this: Element) {
      return offset(this).top;
    }),
    vi.spyOn(Element.prototype, 'scrollTop', 'set').mockImplementation(function (this: Element, top: number) {
      offsets.set(this, { ...offset(this), top });
    }),
    vi.spyOn(Element.prototype, 'scrollLeft', 'get').mockImplementation(function (this: Element) {
      return offset(this).left;
    }),
    vi.spyOn(Element.prototype, 'scrollLeft', 'set').mockImplementation(function (this: Element, left: number) {
      offsets.set(this, { ...offset(this), left });
    }),
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
      putImageData: () => {},
    } as unknown as CanvasRenderingContext2D),
  ];
  vi.stubGlobal('ImageData', class {});
  return () => restore.forEach((spy) => spy.mockRestore());
}

let undoLayout: () => void;
beforeEach(() => {
  undoLayout = fakeLayout();
});
afterEach(() => {
  // Unmount first: a render still in flight must not paint into a real (jsdom) canvas.
  cleanup();
  undoLayout();
});

describe('DocumentViewer', () => {
  it('keeps the page under the viewport mounted through a zoom deep in the document', async () => {
    const { container } = render(<DocumentViewer engine={engine} document={doc} />);
    const scroller = container.querySelector<HTMLElement>('.viewer__scroller')!;

    // Scroll to page 30 and let it render.
    act(() => {
      scroller.scrollTop = 29 * (792 * (96 / 72) + 16);
      fireEvent.scroll(scroller);
    });
    const slot = () => container.querySelector<HTMLElement>('[data-page="30"]');
    await waitFor(() => expect(slot()?.dataset.state).toBe('rendered'));
    const before = slot()!;

    // Ctrl+wheel zooms in a lot around the viewport centre.
    act(() => {
      fireEvent.wheel(scroller, { deltaY: -400, ctrlKey: true, clientX: 400, clientY: 300 });
    });

    // The same slot (and so its bitmap) stayed mounted: no remount, no blank.
    expect(scroller.dataset.zoom).not.toBe('1');
    expect(slot()).toBe(before);
    expect(before.querySelector('canvas')).not.toBeNull();
  });
});
