import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DocumentInfo, RenderedPage } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { createRef } from 'react';
import type { DeliverRequest, Delivery } from '../save/delivery';
import { DocumentWorkspace, type WorkspaceHandle } from './DocumentWorkspace';

const info: DocumentInfo = {
  pageCount: 2,
  pages: Array(2).fill({ widthPt: 612, heightPt: 792, rotationDegrees: 0 }),
  pageIds: [0, 1],
  textExtractionAllowed: true,
  documentAssemblyAllowed: true,
  blocks: [{ source: { kind: 'base' }, part: null, start: 0, count: 2 }],
  importRefusal: null,
  annotationEditingAllowed: true,
  formFillAllowed: true,
};
const page: RenderedPage = { width: 1, height: 1, rgba: new Uint8ClampedArray(4) };
const engine = fakeEngine({
  open: async () => ({ handle: 1, info }),
  renderPage: async () => page,
});

beforeEach(() => {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
    putImageData: () => {},
  } as unknown as CanvasRenderingContext2D);
  vi.stubGlobal('ImageData', class {});
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function openPdf(getByLabelText: (label: string) => HTMLElement) {
  const pdf = new File([new Uint8Array([37, 80, 68, 70])], 'two.pdf', { type: 'application/pdf' });
  fireEvent.change(getByLabelText('Open PDF'), { target: { files: [pdf] } });
}

describe('DocumentWorkspace', () => {
  it('opens a document in the viewer by default', async () => {
    const { getByLabelText, getByRole } = render(<DocumentWorkspace engine={engine} intro={(action) => action} />);
    openPdf(getByLabelText);
    await waitFor(() => expect(getByRole('button', { name: 'View' }).getAttribute('aria-pressed')).toBe('true'));
  });

  it('opens a document in the screen its tool asks for', async () => {
    const { getByLabelText, getByRole } = render(
      <DocumentWorkspace engine={engine} intro={(action) => action} initialScreen="organize" />,
    );
    openPdf(getByLabelText);
    await waitFor(() => expect(getByRole('button', { name: 'Organize' }).getAttribute('aria-pressed')).toBe('true'));
  });

  it('carries the .vitela root class its stylesheet is scoped to', () => {
    const { container } = render(<DocumentWorkspace engine={engine} intro={(action) => action} />);
    expect(container.firstElementChild?.classList.contains('vitela')).toBe(true);
  });

  describe('dropped files', () => {
    /** Drops a PDF on `target` (jsdom has no DragEvent/DataTransfer). */
    function dropOn(target: Node) {
      const pdf = new File([new Uint8Array([37, 80, 68, 70])], 'dropped.pdf', { type: 'application/pdf' });
      for (const type of ['dragenter', 'dragover', 'drop']) {
        const event = new Event(type, { bubbles: true, cancelable: true });
        Object.defineProperty(event, 'dataTransfer', { value: { types: ['Files'], files: [pdf], dropEffect: 'none' } });
        fireEvent(target, event);
      }
    }

    it('opens only the ones dropped on the editor by default', async () => {
      const outside = document.body.appendChild(document.createElement('div'));
      const { container, getByRole, queryByRole } = render(<DocumentWorkspace engine={engine} intro={(action) => action} />);
      dropOn(outside);
      expect(container.querySelector('.drop-hint')).toBeNull();
      // Long enough for an open to land (the fake engine answers at once).
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 20));
      });
      expect(queryByRole('button', { name: 'View' })).toBeNull();
      dropOn(container.firstElementChild!);
      await waitFor(() => expect(getByRole('button', { name: 'View' })).toBeTruthy());
      outside.remove();
    });

    it('opens the ones dropped anywhere on the window when asked to', async () => {
      const outside = document.body.appendChild(document.createElement('div'));
      const { getByRole } = render(<DocumentWorkspace engine={engine} intro={(action) => action} eventScope="window" />);
      dropOn(outside);
      await waitFor(() => expect(getByRole('button', { name: 'View' })).toBeTruthy());
      outside.remove();
    });
  });

  it('can leave the file picker out, before and after a document is open', async () => {
    const workspace = createRef<WorkspaceHandle>();
    const { queryByLabelText, findByText } = render(
      <DocumentWorkspace engine={engine} intro={(action) => action} filePicker={false} ref={workspace} />,
    );
    expect(queryByLabelText('Open PDF')).toBeNull();
    act(() => workspace.current!.open(new File([new Uint8Array([37])], 'two.pdf')));
    expect(await findByText('two.pdf — 2 pages')).toBeTruthy();
    expect(queryByLabelText('Open PDF')).toBeNull();
  });

  it('opens a source and saves through its handle, delivering where it is told', async () => {
    const saved = new Uint8Array([1, 2, 3]);
    const delivered: DeliverRequest[] = [];
    const delivery: Delivery = {
      prepare: async () => ({ kind: 'copy' }),
      deliver: async (request) => (delivered.push(request), { how: 'delivered', name: request.name }),
    };
    const workspace = createRef<WorkspaceHandle>();
    const { findByText } = render(
      <DocumentWorkspace
        engine={fakeEngine({ open: async () => ({ handle: 1, info }), renderPage: async () => page, saveToBytes: async () => saved })}
        intro={(action) => action}
        delivery={delivery}
        ref={workspace}
      />,
    );
    act(() => workspace.current!.open({ name: 'from-host.pdf', arrayBuffer: async () => new ArrayBuffer(4) }));
    expect(await findByText('from-host.pdf — 2 pages')).toBeTruthy();

    act(() => workspace.current!.save());
    expect(await findByText('Saved from-host.pdf.')).toBeTruthy();
    expect(delivered).toEqual([{ bytes: saved, name: 'from-host.pdf', destination: { kind: 'copy' } }]);
  });
});
