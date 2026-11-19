import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { useLayoutEffect } from 'react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { EngineError, EDIT_NOT_PERMITTED } from '@vitela/core';
import type { DocumentHandle, Edit, EditedDocument, Engine, PageSize, RenderedPage } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import { MarkDirtyContext } from '../save/MarkDirtyContext';
import { DocumentViewer } from './DocumentViewer';

// Wide enough for a Letter page at 100%, so documents open at 100% (see initialZoom).
const VIEWPORT = { width: 1000, height: 600 };
const page: RenderedPage = { width: 1, height: 1, rgba: new Uint8ClampedArray(4) };
const pages: PageSize[] = Array(40).fill({ widthPt: 612, heightPt: 792, rotationDegrees: 0 });
const doc: OpenDocument = {
  name: 'long.pdf',
  info: {
    pageCount: pages.length,
    pages,
    pageIds: pages.map((_, index) => index),
    textExtractionAllowed: true,
    documentAssemblyAllowed: true,
    blocks: [],
    importRefusal: null,
    annotationEditingAllowed: true,
  },
  handle: 1,
  generation: 1,
};
/** One Letter page in CSS px at 100%, plus the gap after it. */
const PAGE_STEP = 792 * (96 / 72) + 16;

const engine: Engine = fakeEngine({ renderPage: async () => page });

interface ViewerProps {
  readonly engine: Engine;
  readonly document: OpenDocument;
  readonly onEdited?: (handle: DocumentHandle, edited: EditedDocument) => boolean;
  readonly markDirty?: () => void;
  readonly onStampImage?: (handle: DocumentHandle, id: number, href: string) => void;
}

/** The viewer inside the save state the workspace provides. */
function Viewer({ engine, document, onEdited = () => true, markDirty = () => {}, onStampImage = () => {} }: ViewerProps) {
  return (
    <MarkDirtyContext.Provider value={markDirty}>
      <DocumentViewer engine={engine} document={document} onEdited={onEdited} onStampImage={onStampImage} />
    </MarkDirtyContext.Provider>
  );
}

const edited = (overrides: Partial<EditedDocument> = {}): EditedDocument => ({
  info: doc.info,
  canUndo: true,
  canRedo: false,
  annotations: [],
  addedAnnotationId: null,
  ...overrides,
});

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
  it('follows the scroll after opening at fit width on a narrow viewport', async () => {
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(390);
    const { container, getByText } = render(<Viewer engine={engine} document={doc} />);
    const scroller = container.querySelector<HTMLElement>('.viewer__scroller')!;
    expect(Number(scroller.dataset.zoom)).toBeLessThan(1);

    // Well past page 1 at any fit-width zoom.
    act(() => {
      scroller.scrollTop = 10 * PAGE_STEP;
      fireEvent.scroll(scroller);
    });

    await waitFor(() => expect(getByText(/^Page \d+ of 40$/).textContent).not.toBe('Page 1 of 40'));
  });

  it('keeps the page under the viewport mounted through a zoom deep in the document', async () => {
    const { container } = render(<Viewer engine={engine} document={doc} />);
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
    const { container, getByLabelText } = render(<Viewer engine={{ ...engine, search }} document={doc} />);
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
    const { container, getByLabelText, getByRole } = render(<Viewer engine={engine} document={restricted} />);
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
    const { getByLabelText, getByRole } = render(<Viewer engine={{ ...engine, search }} document={doc} />);
    fireEvent.change(getByLabelText('Find in document'), { target: { value: 'beta' } });
    fireEvent.submit(getByLabelText('Find in document').closest('form')!);
    await waitFor(() => expect(getByLabelText('Find in document')).toHaveProperty('disabled', true));
    expect(getByRole('note')).toBeTruthy();
  });

  it('marks every selected line up as its own edit and reports each one', async () => {
    // A word over two lines on page 1.
    const rects = new Float32Array([72, 100, 200, 18, 72, 130, 150, 18]);
    const selectWord = vi.fn(async () => ({ text: 'two lines', rects }));
    const applyEdit = vi.fn(async (_handle: DocumentHandle, _edit: Edit) => edited());
    const onEdited = vi.fn(() => true);
    const markDirty = vi.fn();
    const { container, getByRole } = render(
      <Viewer engine={{ ...engine, selectWord, applyEdit }} document={doc} onEdited={onEdited} markDirty={markDirty} />,
    );
    const layer = container.querySelector('[data-page="1"] .text-layer')!;
    expect(getByRole('button', { name: 'Highlight' }).getAttribute('aria-disabled')).toBe('true');

    fireEvent.doubleClick(layer, { clientX: 100, clientY: 140 });
    await waitFor(() => expect(container.querySelectorAll('.text-selection')).toHaveLength(2));
    fireEvent.click(getByRole('button', { name: 'Highlight' }));

    await waitFor(() => expect(applyEdit).toHaveBeenCalledTimes(2));
    expect(applyEdit.mock.calls.map(([, edit]) => edit)).toMatchObject([
      { kind: 'addMarkup', page: 0, markup: 'highlight', rect: { left: 72, top: 100, width: 200, height: 18 } },
      { kind: 'addMarkup', page: 0, markup: 'highlight', rect: { left: 72, top: 130, width: 150, height: 18 } },
    ]);
    await waitFor(() => expect(markDirty).toHaveBeenCalledTimes(2));
    expect(onEdited).toHaveBeenCalledWith(1, expect.objectContaining({ canUndo: true }));
    // The marked lines replace the selection.
    await waitFor(() => expect(container.querySelectorAll('.text-selection')).toHaveLength(0));
  });

  it('draws the session’s annotations on their page', async () => {
    const annotated: OpenDocument = {
      ...doc,
      annotations: [
        { id: 3, pageIndex: 1, kind: 'highlight', rect: { left: 72, top: 100, width: 200, height: 18 }, color: { r: 255, g: 220, b: 0 } },
      ],
    };
    const { container } = render(<Viewer engine={engine} document={annotated} />);
    expect(container.querySelector('[data-page="1"] [data-annotation-id]')).toBeNull();
    expect(container.querySelector('[data-page="2"] [data-annotation-id="3"]')).not.toBeNull();
  });

  it('undoes and redoes from the toolbar and the keyboard', async () => {
    const undo = vi.fn(async () => edited({ canUndo: false, canRedo: true }));
    const redo = vi.fn(async () => edited());
    const markDirty = vi.fn();
    const history: OpenDocument = { ...doc, canUndo: true, canRedo: true };
    const { getByRole } = render(<Viewer engine={{ ...engine, undo, redo }} document={history} markDirty={markDirty} />);

    fireEvent.click(getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(undo).toHaveBeenCalledTimes(1));
    fireEvent.click(getByRole('button', { name: 'Redo' }));
    await waitFor(() => expect(redo).toHaveBeenCalledTimes(1));
    fireEvent.keyDown(document.body, { key: 'z', ctrlKey: true });
    await waitFor(() => expect(undo).toHaveBeenCalledTimes(2));
    fireEvent.keyDown(document.body, { key: 'Z', ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(redo).toHaveBeenCalledTimes(2));
    // The search box keeps Ctrl+Z for its own text.
    fireEvent.keyDown(getByRole('searchbox'), { key: 'z', ctrlKey: true });
    await waitFor(() => expect(markDirty).toHaveBeenCalledTimes(4));
    expect(undo).toHaveBeenCalledTimes(2);
  });

  it('offers no undo before the document has a history', () => {
    const undo = vi.fn();
    const { getByRole } = render(<Viewer engine={{ ...engine, undo }} document={doc} />);
    expect(getByRole('button', { name: 'Undo' }).getAttribute('aria-disabled')).toBe('true');
    fireEvent.click(getByRole('button', { name: 'Undo' }));
    fireEvent.keyDown(document.body, { key: 'z', ctrlKey: true });
    expect(undo).not.toHaveBeenCalled();
  });

  it('explains why markup is off for a document that forbids annotations', () => {
    const locked = { ...doc, info: { ...doc.info, annotationEditingAllowed: false } };
    const { getByRole } = render(<Viewer engine={engine} document={locked} />);
    const highlight = getByRole('button', { name: 'Highlight' });
    expect(highlight.getAttribute('aria-disabled')).toBe('true');
    expect(document.getElementById(highlight.getAttribute('aria-describedby')!)?.textContent).toMatch(
      /does not allow annotations/,
    );
  });

  it('turns markup off when the core refuses it', async () => {
    const selectWord = vi.fn(async () => ({ text: 'word', rects: new Float32Array([72, 100, 40, 18]) }));
    const applyEdit = vi.fn(async () => {
      throw new EngineError(EDIT_NOT_PERMITTED, 'annotating is not permitted');
    });
    const { container, getByRole } = render(<Viewer engine={{ ...engine, selectWord, applyEdit }} document={doc} />);
    fireEvent.doubleClick(container.querySelector('[data-page="1"] .text-layer')!, { clientX: 100, clientY: 140 });
    await waitFor(() => expect(container.querySelectorAll('.text-selection')).toHaveLength(1));
    fireEvent.click(getByRole('button', { name: 'Underline' }));

    await waitFor(() => expect(getByRole('alert').textContent).toBe('annotating is not permitted'));
    expect(getByRole('button', { name: 'Underline' }).getAttribute('aria-describedby')).not.toBeNull();
    // Nothing was marked: the selection stays.
    expect(container.querySelectorAll('.text-selection')).toHaveLength(1);
  });

  it('turns markup off while an undo is still running', async () => {
    const selectWord = vi.fn(async () => ({ text: 'word', rects: new Float32Array([72, 100, 40, 18]) }));
    let release: () => void = () => {};
    const undo = vi.fn(
      () =>
        new Promise<EditedDocument>((resolve) => {
          release = () => resolve(edited({ canUndo: false, canRedo: true }));
        }),
    );
    const applyEdit = vi.fn(async () => edited());
    const history: OpenDocument = { ...doc, canUndo: true };
    const { container, getByRole } = render(<Viewer engine={{ ...engine, selectWord, undo, applyEdit }} document={history} />);
    fireEvent.doubleClick(container.querySelector('[data-page="1"] .text-layer')!, { clientX: 100, clientY: 140 });
    await waitFor(() => expect(container.querySelectorAll('.text-selection')).toHaveLength(1));
    const highlight = () => getByRole('button', { name: 'Highlight' });
    expect(highlight().getAttribute('aria-disabled')).toBe('false');

    fireEvent.click(getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(undo).toHaveBeenCalledTimes(1));
    expect(highlight().getAttribute('aria-disabled')).toBe('true');
    fireEvent.click(highlight());

    await act(async () => release());
    await waitFor(() => expect(highlight().getAttribute('aria-disabled')).toBe('false'));
    expect(applyEdit).not.toHaveBeenCalled();
  });

  it('keeps the error, and drops the selection, when a later line of a markup fails', async () => {
    const rects = new Float32Array([72, 100, 200, 18, 72, 130, 150, 18]);
    const selectWord = vi.fn(async () => ({ text: 'two lines', rects }));
    const applyEdit = vi
      .fn()
      .mockResolvedValueOnce(edited())
      .mockRejectedValueOnce(new Error('worker gone'));
    const { container, getByRole } = render(<Viewer engine={{ ...engine, selectWord, applyEdit }} document={doc} />);
    fireEvent.doubleClick(container.querySelector('[data-page="1"] .text-layer')!, { clientX: 100, clientY: 140 });
    await waitFor(() => expect(container.querySelectorAll('.text-selection')).toHaveLength(2));
    fireEvent.click(getByRole('button', { name: 'Highlight' }));

    await waitFor(() => expect(getByRole('alert').textContent).toBe('worker gone'));
    expect(applyEdit).toHaveBeenCalledTimes(2);
    // The first line went in: marking the selection again would add it twice.
    await waitFor(() => expect(container.querySelectorAll('.text-selection')).toHaveLength(0));
    expect(getByRole('alert').textContent).toBe('worker gone');
  });

  it('marks nothing when the page list changed after the text was selected', async () => {
    const selectWord = vi.fn(async () => ({ text: 'word', rects: new Float32Array([72, 100, 40, 18]) }));
    const applyEdit = vi.fn(async () => edited());
    const viewerEngine = { ...engine, selectWord, applyEdit };
    /**
     * Clicks Highlight as soon as the new page list is on screen, before the
     * viewer's effects drop the old selection: the window a real click can hit.
     */
    function ClickHighlightOnChange({ document: shown }: { readonly document: OpenDocument }) {
      useLayoutEffect(() => {
        if (shown === doc) return;
        const buttons = window.document.querySelectorAll<HTMLButtonElement>('.markup-button');
        [...buttons].find((button) => button.textContent === 'Highlight')?.click();
      }, [shown]);
      return null;
    }
    const view = (shown: OpenDocument) => (
      <>
        <Viewer engine={viewerEngine} document={shown} />
        <ClickHighlightOnChange document={shown} />
      </>
    );
    const { container, getByRole, rerender } = render(view(doc));
    fireEvent.doubleClick(container.querySelector('[data-page="1"] .text-layer')!, { clientX: 100, clientY: 140 });
    await waitFor(() => expect(container.querySelectorAll('.text-selection')).toHaveLength(1));
    expect(getByRole('button', { name: 'Highlight' }).getAttribute('aria-disabled')).toBe('false');

    // Pages 1 and 2 swapped places (an undo of a move made in Organize).
    const swapped: OpenDocument = { ...doc, info: { ...doc.info, pageIds: [1, 0, ...doc.info.pageIds.slice(2)] } };
    rerender(view(swapped));

    await waitFor(() => expect(getByRole('alert').textContent).toMatch(/page changed/i));
    expect(applyEdit).not.toHaveBeenCalled();
  });

  it('keeps a selection made while the markup ran', async () => {
    const first = new Float32Array([72, 100, 40, 18]);
    const second = new Float32Array([72, 300, 60, 18]);
    const selectWord = vi.fn().mockResolvedValueOnce({ text: 'word', rects: first }).mockResolvedValueOnce({ text: 'other', rects: second });
    let release: () => void = () => {};
    const applyEdit = vi.fn(
      () =>
        new Promise<EditedDocument>((resolve) => {
          release = () => resolve(edited());
        }),
    );
    const { container, getByRole } = render(<Viewer engine={{ ...engine, selectWord, applyEdit }} document={doc} />);
    const layer = () => container.querySelector('[data-page="1"] .text-layer')!;
    fireEvent.doubleClick(layer(), { clientX: 100, clientY: 140 });
    await waitFor(() => expect(container.querySelectorAll('.text-selection')).toHaveLength(1));
    fireEvent.click(getByRole('button', { name: 'Highlight' }));
    await waitFor(() => expect(applyEdit).toHaveBeenCalledTimes(1));

    // A new word selected while the core marks the first one.
    fireEvent.doubleClick(layer(), { clientX: 100, clientY: 420 });
    await waitFor(() => expect(selectWord).toHaveBeenCalledTimes(2));
    const shownTop = () => (container.querySelector<HTMLElement>('.text-selection')?.style.top ?? '');
    await waitFor(() => expect(shownTop()).toBe(`${300 * (96 / 72)}px`));

    await act(async () => release());
    await waitFor(() => expect(container.querySelector('.viewer__failure')).toBeNull());
    expect(container.querySelectorAll('.text-selection')).toHaveLength(1);
    expect(shownTop()).toBe(`${300 * (96 / 72)}px`);
  });

  it('repaints a position, and drops the selection, when an undo puts another page there', async () => {
    const renderPage = vi.fn(async () => page);
    const selectWord = vi.fn(async () => ({ text: 'word', rects: new Float32Array([72, 100, 40, 18]) }));
    const { container, rerender } = render(<Viewer engine={{ ...engine, renderPage, selectWord }} document={doc} />);
    await waitFor(() => expect(container.querySelector('[data-page="1"]')?.getAttribute('data-state')).toBe('rendered'));
    fireEvent.doubleClick(container.querySelector('[data-page="1"] .text-layer')!, { clientX: 100, clientY: 140 });
    await waitFor(() => expect(container.querySelectorAll('.text-selection')).toHaveLength(1));
    const firstPageRenders = () => renderPage.mock.calls.filter((call) => (call as unknown[])[1] === 0).length;
    const before = firstPageRenders();

    // Pages 1 and 2 swapped places (an undo of a move made in Organize).
    const swapped: OpenDocument = { ...doc, info: { ...doc.info, pageIds: [1, 0, ...doc.info.pageIds.slice(2)] } };
    rerender(<Viewer engine={{ ...engine, renderPage, selectWord }} document={swapped} />);

    await waitFor(() => expect(firstPageRenders()).toBe(before + 1));
    await waitFor(() => expect(container.querySelectorAll('.text-selection')).toHaveLength(0));
  });
});

describe('DocumentViewer drawing tools', () => {
  beforeAll(() => {
    HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
      this.setAttribute('open', '');
    };
    HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
      this.removeAttribute('open');
    };
  });

  /** CSS px per point at 100%: pointer offsets in the (unlaid-out) slot are these × points. */
  const PX = 96 / 72;
  const at = (x: number, y: number) => ({ pointerId: 1, button: 0, isPrimary: true, clientX: x * PX, clientY: y * PX });

  it('draws an ink stroke as one edit, with text selection off while the tool is armed', async () => {
    const applyEdit = vi.fn(async (_handle: DocumentHandle, _edit: Edit) => edited());
    const markDirty = vi.fn();
    const { container, getByRole } = render(<Viewer engine={{ ...engine, applyEdit }} document={doc} markDirty={markDirty} />);
    const page = () => container.querySelector('[data-page="1"]')!;
    expect(page().querySelector('.drawing-layer')).toBeNull();

    fireEvent.click(getByRole('button', { name: 'Ink' }));
    expect(getByRole('button', { name: 'Ink' }).getAttribute('aria-pressed')).toBe('true');
    expect(page().querySelector('.text-layer')?.getAttribute('data-selectable')).toBe('false');
    const surface = page().querySelector('.drawing-layer')!;
    fireEvent.pointerDown(surface, at(30, 60));
    fireEvent.pointerMove(surface, at(90, 60));
    fireEvent.pointerUp(surface, at(90, 120));

    await waitFor(() => expect(markDirty).toHaveBeenCalledTimes(1));
    const edit = applyEdit.mock.calls[0]![1];
    expect(edit).toMatchObject({ kind: 'addInk', page: 0, color: { r: 255, g: 220, b: 0 } });
    const points = (edit as Extract<Edit, { kind: 'addInk' }>).points.map(({ x, y }) => [Math.round(x), Math.round(y)]);
    expect(points).toEqual([
      [30, 60],
      [90, 60],
      [90, 120],
    ]);

    // Escape goes back to selecting text.
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    });
    expect(page().querySelector('.drawing-layer')).toBeNull();
    expect(page().querySelector('.text-layer')?.getAttribute('data-selectable')).toBe('true');
  });

  it('asks for a note\u2019s text and adds it where it was clicked', async () => {
    const applyEdit = vi.fn(async (_handle: DocumentHandle, _edit: Edit) => edited());
    const { container, getByRole, queryByRole } = render(<Viewer engine={{ ...engine, applyEdit }} document={doc} />);
    fireEvent.click(getByRole('button', { name: 'Note' }));
    const surface = container.querySelector('[data-page="2"] .drawing-layer')!;
    fireEvent.pointerDown(surface, at(300, 300));
    fireEvent.pointerUp(surface, at(300, 300));

    const field = getByRole('dialog', { name: 'Add a note' }).querySelector('textarea')!;
    fireEvent.change(field, { target: { value: 'Check this' } });
    fireEvent.click(getByRole('button', { name: 'Add note' }));

    await waitFor(() => expect(queryByRole('dialog')).toBeNull());
    const edit = applyEdit.mock.calls[0]![1];
    expect(edit).toMatchObject({ kind: 'addNote', page: 1, contents: 'Check this' });
  });

  it('keeps the tools off, and says why, for a document that forbids annotations', () => {
    const locked = { ...doc, info: { ...doc.info, annotationEditingAllowed: false } };
    const { container, getByRole } = render(<Viewer engine={engine} document={locked} />);
    const ink = getByRole('button', { name: 'Ink' });
    expect(ink.getAttribute('aria-disabled')).toBe('true');
    expect(document.getElementById(ink.getAttribute('aria-describedby')!)?.textContent).toMatch(/does not allow annotations/);
    fireEvent.click(ink);
    expect(container.querySelector('.drawing-layer')).toBeNull();
  });

  const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const BOX = { left: 40, top: 60, width: 144, height: 72 };
  const png = () => new File([PNG], 'logo.png', { type: 'image/png' });

  it('arms Image with a picked PNG and stamps it where the page is clicked, keeping its image by id', async () => {
    const stampPlacement = vi.fn(async (_handle: DocumentHandle, _page: number, _image: Uint8Array, _anchor: { x: number; y: number }) => BOX);
    const applyEdit = vi.fn(async (_handle: DocumentHandle, _edit: Edit) => edited({ addedAnnotationId: 5 }));
    const onStampImage = vi.fn();
    const { container, getByRole, getByLabelText } = render(
      <Viewer engine={{ ...engine, applyEdit, stampPlacement }} document={doc} onStampImage={onStampImage} />,
    );
    fireEvent.change(getByLabelText('Stamp image'), { target: { files: [png()] } });
    await waitFor(() => expect(getByRole('button', { name: 'Image' }).getAttribute('aria-pressed')).toBe('true'));

    const surface = container.querySelector('[data-page="2"] .drawing-layer')!;
    fireEvent.pointerDown(surface, at(30, 60));
    fireEvent.pointerUp(surface, at(30, 60));

    await waitFor(() => expect(onStampImage).toHaveBeenCalledTimes(1));
    const [, page, image, anchor] = stampPlacement.mock.calls[0]!;
    expect(page).toBe(1);
    expect([...image]).toEqual([...PNG]);
    expect([Math.round(anchor.x), Math.round(anchor.y)]).toEqual([30, 60]);
    expect(applyEdit.mock.calls[0]![1]).toEqual({ kind: 'addStamp', page: 1, rect: BOX, image });
    expect(onStampImage).toHaveBeenCalledWith(1, 5, expect.stringMatching(/^data:image\/png;base64,/));
  });

  it('stamps a PNG dropped on a page, claiming the drop so the workspace does not open it', async () => {
    const stampPlacement = vi.fn(async () => BOX);
    const applyEdit = vi.fn(async (_handle: DocumentHandle, _edit: Edit) => edited({ addedAnnotationId: 2 }));
    const onStampImage = vi.fn();
    const { container } = render(
      <Viewer engine={{ ...engine, applyEdit, stampPlacement }} document={doc} onStampImage={onStampImage} />,
    );
    const slot = container.querySelector('[data-page="1"]')!;
    const claimed = !fireEvent.drop(slot, { dataTransfer: { files: [png()], types: ['Files'] } });
    expect(claimed).toBe(true);
    await waitFor(() => expect(onStampImage).toHaveBeenCalledWith(1, 2, expect.stringMatching(/^data:image\/png;base64,/)));
    expect(applyEdit.mock.calls[0]![1]).toMatchObject({ kind: 'addStamp', page: 0, rect: BOX });

    // A PDF dropped on a page is the workspace's to open.
    const pdf = new File(['%PDF-1.7'], 'other.pdf', { type: 'application/pdf' });
    expect(fireEvent.drop(slot, { dataTransfer: { files: [pdf], types: ['Files'] } })).toBe(true);
  });

  it('draws a session stamp with the image the workspace kept for it', () => {
    const stamped = {
      ...doc,
      annotations: [{ id: 5, pageIndex: 0, kind: 'stamp' as const, rect: BOX }],
      stampImages: new Map([[5, 'data:image/png;base64,iVBORw==']]),
    };
    const { container } = render(<Viewer engine={engine} document={stamped} />);
    expect(container.querySelector('[data-page="1"] [data-annotation-id="5"] image')?.getAttribute('href')).toBe(
      'data:image/png;base64,iVBORw==',
    );
  });
});
