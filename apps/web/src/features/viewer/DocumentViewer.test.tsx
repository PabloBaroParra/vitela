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
/** One Letter page in CSS px at 100%, plus the gap after it. */
const PAGE_STEP = 792 * (96 / 72) + 16;

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
      scroller.scrollTop = 29 * PAGE_STEP;
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

  it('scrolls a match on a page that is not mounted into view and highlights it', async () => {
    // "beta" on page 31, 400 pt down it (drawn points from the top).
    const search = vi.fn(async () => [{ pageIndex: 30, rects: new Float32Array([72, 400, 30, 12]) }]);
    const { container, getByLabelText } = render(<DocumentViewer engine={{ ...engine, search }} document={doc} />);
    const scroller = container.querySelector<HTMLElement>('.viewer__scroller')!;
    expect(container.querySelector('[data-page="31"]')).toBeNull();

    fireEvent.change(getByLabelText('Find in document'), { target: { value: 'beta' } });
    fireEvent.submit(getByLabelText('Find in document').closest('form')!);

    await waitFor(() => expect(getByLabelText('Search results').textContent).toBe('1 of 1'));
    expect(search).toHaveBeenCalledWith(1, 'beta');
    // The match (16 px padding + 30 pages + 400 pt) sits mid-viewport.
    const matchTop = 16 + 30 * PAGE_STEP + 400 * (96 / 72);
    expect(scroller.scrollTop).toBeCloseTo(matchTop + 8 - VIEWPORT.height / 2, 0);
    act(() => {
      fireEvent.scroll(scroller);
    });
    await waitFor(() => expect(container.querySelector('[data-page="31"] .search-hit--current')).not.toBeNull());
  });

  it('turns search and selection off for a document that withholds its text', () => {
    const restricted = { ...doc, info: { ...doc.info, textExtractionAllowed: false } };
    const { container, getByLabelText, getByRole } = render(<DocumentViewer engine={engine} document={restricted} />);
    expect(getByLabelText('Find in document')).toHaveProperty('disabled', true);
    expect(getByRole('note').textContent).toMatch(/does not allow copying its text/);
    expect(getByRole('button', { name: 'Copy' })).toHaveProperty('disabled', true);
    const layer = container.querySelector('[data-page="1"] .text-layer')!;
    expect(layer.getAttribute('data-selectable')).toBe('false');
  });

  it('turns text off when the engine refuses a read the open said was allowed', async () => {
    const search = vi.fn(async () => {
      throw Object.assign(new Error('text extraction is not permitted'), { name: 'TextExtractionDenied' });
    });
    const { getByLabelText, getByRole } = render(<DocumentViewer engine={{ ...engine, search }} document={doc} />);
    fireEvent.change(getByLabelText('Find in document'), { target: { value: 'beta' } });
    fireEvent.submit(getByLabelText('Find in document').closest('form')!);
    await waitFor(() => expect(getByLabelText('Find in document')).toHaveProperty('disabled', true));
    expect(getByRole('note')).toBeTruthy();
  });
});
