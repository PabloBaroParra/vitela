import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DocumentInfo, DocumentProperties, RenderedPage } from '@vitela/core';
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

type ByRole = (role: string, options: { name: string }) => HTMLElement;

/** Opens `tool` from the bar's Tools menu, once the document is open. */
async function chooseTool(getByRole: ByRole, tool: string) {
  fireEvent.click(await waitFor(() => getByRole('button', { name: 'Tools' })));
  fireEvent.click(getByRole('menuitem', { name: tool }));
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

  it('offers Compress in the Tools menu in both screens, delivering the smaller copy where saves go', async () => {
    HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
      this.setAttribute('open', '');
    };
    HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
      this.removeAttribute('open');
    };
    const delivered: DeliverRequest[] = [];
    const delivery: Delivery = {
      prepare: async () => ({ kind: 'copy' }),
      deliver: async (request) => (delivered.push(request), { how: 'delivered', name: request.name }),
    };
    const bytes = new Uint8Array([9]);
    const compressing = fakeEngine({
      open: async () => ({ handle: 1, info }),
      renderPage: async () => page,
      compressPresets: async () => ['lossless', 'balanced', 'small'],
      compressionRefusal: async () => null,
      compressionInvalidatesSignatures: async () => false,
      saveCompressedToBytes: async () => ({ bytes, beforeBytes: 4000, afterBytes: 1000, savedBytes: 3000, reduced: true, refusals: [] }),
    });
    const { getByLabelText, getByRole, findByRole, findByText } = render(
      <DocumentWorkspace engine={compressing} intro={(action) => action} initialScreen="organize" delivery={delivery} />,
    );
    openPdf(getByLabelText);
    await waitFor(() => expect(getByRole('button', { name: 'Organize' }).getAttribute('aria-pressed')).toBe('true'));

    await chooseTool(getByRole, 'Compress');
    const dialog = await findByRole('dialog', { name: 'Compress PDF' });
    fireEvent.click(dialog.querySelector('button[type="submit"]')!);
    expect(await findByText('4 kB uncompressed → 1 kB compressed (75% smaller). Saved two-compressed.pdf.')).toBeTruthy();
    expect(delivered).toEqual([{ bytes, name: 'two-compressed.pdf', destination: { kind: 'newFile' } }]);
  });

  it('offers Export images in the Tools menu, delivering each page where saves go', async () => {
    HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
      this.setAttribute('open', '');
    };
    HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
      this.removeAttribute('open');
    };
    const delivered: DeliverRequest[] = [];
    const delivery: Delivery = {
      prepare: async () => ({ kind: 'copy' }),
      deliver: async (request) => (delivered.push(request), { how: 'delivered', name: request.name }),
    };
    const exporting = fakeEngine({
      open: async () => ({ handle: 1, info }),
      renderPage: async () => page,
      firstPageTooLargeToExport: async () => null,
      pageImageFileNames: async (_name, pages) => pages.map((index) => `two-${index + 1}.png`),
      imageExportFormats: async () => ['png', 'jpeg'],
      openImageExport: async () => ({ handle: 3, pageCount: 2 }),
      exportPageImage: async (_export, index) => new Uint8Array([index]),
      closeImageExport: async () => true,
    });
    const { getByLabelText, getByRole, findByRole, findByText } = render(
      <DocumentWorkspace engine={exporting} intro={(action) => action} initialScreen="organize" delivery={delivery} />,
    );
    openPdf(getByLabelText);
    await waitFor(() => expect(getByRole('button', { name: 'Organize' }).getAttribute('aria-pressed')).toBe('true'));

    await chooseTool(getByRole, 'Export images');
    const dialog = await findByRole('dialog', { name: 'Export pages as images' });
    fireEvent.click(dialog.querySelector('button[type="submit"]')!);
    expect(await findByText('Exported 2 pages as PNG.')).toBeTruthy();
    expect(delivered.map(({ name, type, destination }) => [name, type, destination.kind])).toEqual([
      ['two-1.png', 'image/png', 'newFile'],
      ['two-2.png', 'image/png', 'newFile'],
    ]);
  });

  it('offers Protect in the Tools menu, delivering the protected copy where saves go after the queued edits', async () => {
    HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
      this.setAttribute('open', '');
    };
    HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
      this.removeAttribute('open');
    };
    const delivered: DeliverRequest[] = [];
    const delivery: Delivery = {
      prepare: async () => ({ kind: 'copy' }),
      deliver: async (request) => (delivered.push(request), { how: 'delivered', name: request.name }),
    };
    const bytes = new Uint8Array([7]);
    const protecting = fakeEngine({
      open: async () => ({ handle: 1, info }),
      renderPage: async () => page,
      protectionRefusal: async () => null,
      protectionInvalidatesSignatures: async () => false,
      protectToBytes: async () => bytes,
    });
    const { getByLabelText, getByRole, findByRole, findByText } = render(
      <DocumentWorkspace engine={protecting} intro={(action) => action} initialScreen="organize" delivery={delivery} />,
    );
    openPdf(getByLabelText);
    await waitFor(() => expect(getByRole('button', { name: 'Organize' }).getAttribute('aria-pressed')).toBe('true'));

    await chooseTool(getByRole, 'Protect');
    const dialog = await findByRole('dialog', { name: 'Protect with a password' });
    fireEvent.change(getByLabelText('Password to open the document'), { target: { value: 'o' } });
    fireEvent.change(getByLabelText('Permissions password'), { target: { value: 'p' } });
    fireEvent.click(dialog.querySelector('button[type="submit"]')!);
    expect(await findByText('Saved two-protected.pdf, protected with a password. The open document is not protected.')).toBeTruthy();
    expect(delivered).toEqual([{ bytes, name: 'two-protected.pdf', destination: { kind: 'newFile' } }]);
  });

  it('edits the document properties from the Tools menu as an undoable edit that Save writes', async () => {
    HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
      this.setAttribute('open', '');
    };
    HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
      this.removeAttribute('open');
    };
    const none: DocumentProperties = { title: null, author: null, subject: null, keywords: null, creator: null, producer: null };
    let properties = none;
    const edited = { info, annotations: [], addedAnnotationId: null, formFields: [] };
    const describing = fakeEngine({
      open: async () => ({ handle: 1, info }),
      renderPage: async () => page,
      documentProperties: async () => properties,
      metadataRefusal: async () => null,
      applyEdit: async (_handle, edit) => {
        if (edit.kind === 'setDocumentProperties') properties = edit.properties;
        return { ...edited, canUndo: true, canRedo: false };
      },
      undo: async () => ((properties = none), { ...edited, canUndo: false, canRedo: true }),
    });
    const { getByLabelText, getByRole, findByRole, findByText } = render(
      <DocumentWorkspace engine={describing} intro={(action) => action} />,
    );
    openPdf(getByLabelText);
    await chooseTool(getByRole, 'Document properties');
    const dialog = await findByRole('dialog', { name: 'Document properties' });
    fireEvent.change(getByLabelText('Title'), { target: { value: 'Annual report' } });
    fireEvent.click(dialog.querySelector('button[type="submit"]')!);
    expect(await findByText('Document properties updated. Changes are pending save.')).toBeTruthy();
    expect(await findByText('Unsaved changes.')).toBeTruthy();
    expect(properties).toMatchObject({ title: 'Annual report' });

    // The change is in the viewer's history: Undo takes it back.
    const undo = getByRole('button', { name: 'Undo' }) as HTMLButtonElement;
    await waitFor(() => expect(undo.disabled).toBe(false));
    fireEvent.click(undo);
    await waitFor(() => expect(properties).toBe(none));
  });

  it('stamps a watermark from the Tools menu as an undoable edit, and gives the focus back to Tools', async () => {
    HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
      this.setAttribute('open', '');
    };
    HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
      this.removeAttribute('open');
    };
    const applied: unknown[] = [];
    const edited = { info, annotations: [], addedAnnotationId: null, formFields: [] };
    const stamping = fakeEngine({
      open: async () => ({ handle: 1, info }),
      renderPage: async () => page,
      watermarkDefaults: async () => ({
        fontSize: 60,
        textOpacity: 0.3,
        textRotationDegrees: 45,
        textBehind: false,
        pictureScale: 0.5,
        pictureOpacity: 0.3,
        pictureRotationDegrees: 0,
        pictureBehind: true,
      }),
      watermarkRefusal: async () => null,
      applyEdit: async (_handle, edit) => (applied.push(edit), { ...edited, canUndo: true, canRedo: false }),
    });
    const { getByLabelText, getByRole, findByRole, findByText } = render(
      <DocumentWorkspace engine={stamping} intro={(action) => action} />,
    );
    openPdf(getByLabelText);
    await chooseTool(getByRole, 'Watermark');
    const dialog = await findByRole('dialog', { name: 'Add watermark' });
    fireEvent.click(dialog.querySelector('button[type="submit"]')!);
    expect(await findByText('Watermark added to 2 pages.')).toBeTruthy();
    expect(await findByText('Unsaved changes.')).toBeTruthy();
    expect(applied).toEqual([
      {
        kind: 'addWatermark',
        pages: [0, 1],
        mark: { kind: 'text', text: 'DRAFT', fontSize: 60 },
        opacity: 0.3,
        rotationDegrees: 45,
        behind: false,
      },
    ]);
    await waitFor(() => expect(getByRole('button', { name: 'Tools' })).toBe(document.activeElement));
    const undo = getByRole('button', { name: 'Undo' }) as HTMLButtonElement;
    await waitFor(() => expect(undo.disabled).toBe(false));
  });

  it("extracts Organize's selection and splits from Organize, each file delivered where saves go", async () => {
    HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
      this.setAttribute('open', '');
    };
    HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
      this.removeAttribute('open');
    };
    const delivered: DeliverRequest[] = [];
    const delivery: Delivery = {
      prepare: async () => ({ kind: 'copy' }),
      deliver: async (request) => (delivered.push(request), { how: 'delivered', name: request.name }),
    };
    const extracted: number[][] = [];
    const extracting = fakeEngine({
      open: async () => ({ handle: 1, info }),
      renderPage: async () => page,
      extractionRefusal: async () => null,
      extractSourceIsSigned: async () => false,
      parsePageSelection: async (input) => (input === '1' ? [0] : [0, 1]),
      extractPages: async (_handle, pages) => (extracted.push([...pages]), new Uint8Array(pages)),
      planSplit: async () => [
        { first: 0, last: 0, fileName: 'two-part1.pdf' },
        { first: 1, last: 1, fileName: 'two-part2.pdf' },
      ],
    });
    const { getByLabelText, getByRole, findByRole, findByText } = render(
      <DocumentWorkspace engine={extracting} intro={(action) => action} initialScreen="organize" delivery={delivery} />,
    );
    openPdf(getByLabelText);
    await waitFor(() => expect(getByRole('button', { name: 'Organize' }).getAttribute('aria-pressed')).toBe('true'));

    // Organize's Extract opens the dialog on its selection (the first page, as opened).
    fireEvent.click(getByRole('button', { name: 'Extract' }));
    const extractDialog = await findByRole('dialog', { name: 'Extract pages' });
    expect((getByLabelText('Pages to extract') as HTMLInputElement).value).toBe('1');
    fireEvent.click(extractDialog.querySelector('button[type="submit"]')!);
    expect(await findByText('Saved two-extract.pdf: 1 page extracted to a new PDF.')).toBeTruthy();

    fireEvent.click(getByRole('button', { name: 'Split' }));
    const splitDialog = await findByRole('dialog', { name: 'Split PDF' });
    fireEvent.change(getByLabelText('Split after page'), { target: { value: '1' } });
    fireEvent.click(splitDialog.querySelector('button[type="submit"]')!);
    expect(await findByText('Split into 2 PDFs.')).toBeTruthy();
    expect(extracted).toEqual([[0], [0], [1]]);
    expect(delivered.map(({ name, destination }) => [name, destination.kind])).toEqual([
      ['two-extract.pdf', 'newFile'],
      ['two-part1.pdf', 'newFile'],
      ['two-part2.pdf', 'newFile'],
    ]);
  });

  it('saves only once a pending Organize edit has landed, and keeps Extract and Split off until then', async () => {
    let land: () => void = () => {};
    const steps: string[] = [];
    const three: DocumentInfo = { ...info, pageCount: 3, pageIds: [0, 2, 1], pages: Array(3).fill(info.pages[0]) };
    const organizing = fakeEngine({
      open: async () => ({ handle: 1, info }),
      renderPage: async () => page,
      applyEdit: async () => {
        await new Promise<void>((resolve) => (land = resolve));
        steps.push('edit');
        return { info: three, canUndo: true, canRedo: false, annotations: [], formFields: [], addedAnnotationId: null };
      },
      saveToBytes: async () => (steps.push('save'), new Uint8Array([1])),
    });
    const delivery: Delivery = {
      prepare: async () => ({ kind: 'copy' }),
      deliver: async (request) => ({ how: 'delivered', name: request.name }),
    };
    const { getByLabelText, getByRole, findByRole } = render(
      <DocumentWorkspace engine={organizing} intro={(action) => action} initialScreen="organize" delivery={delivery} />,
    );
    openPdf(getByLabelText);
    await waitFor(() => expect(getByRole('button', { name: 'Organize' }).getAttribute('aria-pressed')).toBe('true'));
    fireEvent.click(getByRole('button', { name: 'Add blank page' }));
    fireEvent.click(await findByRole('menuitem', { name: 'Portrait A4' }));
    await waitFor(() => expect(getByRole('button', { name: 'Split' }).getAttribute('aria-disabled')).toBe('true'));
    expect(getByRole('button', { name: 'Extract' }).getAttribute('aria-disabled')).toBe('true');

    fireEvent.click(getByRole('button', { name: 'Save' }));
    await act(async () => new Promise((resolve) => setTimeout(resolve, 20)));
    expect(steps).toEqual([]);
    await act(async () => land());
    await waitFor(() => expect(steps).toEqual(['edit', 'save']));
    await waitFor(() => expect(getByRole('button', { name: 'Split' }).getAttribute('aria-disabled')).toBe('false'));
  });

  it('offers the document tools in one Tools menu, placed in the bar before Save, with no button of their own', async () => {
    HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
      this.setAttribute('open', '');
    };
    HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
      this.removeAttribute('open');
    };
    const { getByLabelText, getByRole, findByRole, queryByRole } = render(
      <DocumentWorkspace engine={engine} intro={(action) => action} initialScreen="organize" />,
    );
    openPdf(getByLabelText);
    const tools = await waitFor(() => getByRole('button', { name: 'Tools' }));
    // DOM order is the order on screen: the menu sits before Save, and the tools have no buttons of their own.
    const bar = tools.closest('.workspace__bar')!;
    const order = [...bar.querySelectorAll('button')].map((button) => button.getAttribute('aria-label') ?? button.textContent);
    expect(order.indexOf('Tools')).toBeGreaterThan(order.indexOf('Organize'));
    expect(order.indexOf('Tools')).toBeLessThan(order.indexOf('Save'));
    for (const tool of ['Compress', 'Export images', 'Protect', 'Watermark', 'Properties', 'Document properties']) {
      expect(order).not.toContain(tool);
    }

    fireEvent.click(tools);
    const menu = getByRole('menu', { name: 'Tools' });
    expect([...menu.querySelectorAll('[role="menuitem"]')].map((item) => item.textContent)).toEqual([
      'Compress',
      'Export images',
      'Protect',
      'Watermark',
      'Document properties',
    ]);
    fireEvent.click(getByRole('menuitem', { name: 'Export images' }));
    expect(queryByRole('menu')).toBeNull();
    expect(await findByRole('dialog', { name: 'Export pages as images' })).toBeTruthy();
    // Nothing in the viewer's View toolbar opens a tool any more.
    expect(queryByRole('button', { name: 'Properties' })).toBeNull();
  });
});
