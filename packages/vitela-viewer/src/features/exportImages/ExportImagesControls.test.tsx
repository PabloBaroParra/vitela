import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { EngineError, type Engine } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import type { DeliverRequest, Delivery } from '../save/delivery';
import { ExportImagesControls } from './ExportImagesControls';
import { useExportImages } from './useExportImages';

function openDocument(textExtractionAllowed = true): OpenDocument {
  return {
    name: 'a.pdf',
    handle: 7,
    generation: 1,
    info: {
      pageCount: 3,
      pages: [0, 1, 2].map(() => ({ widthPt: 612, heightPt: 792, rotationDegrees: 0 })),
      pageIds: [0, 1, 2],
      textExtractionAllowed,
      documentAssemblyAllowed: true,
      blocks: [],
      importRefusal: null,
      annotationEditingAllowed: true,
      formFillAllowed: true,
    },
  };
}

beforeAll(() => {
  HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
    this.removeAttribute('open');
  };
});
afterEach(cleanup);

/** A delivery that records what it was handed; `fail` names a file whose write rejects. */
function recording(fail?: string) {
  const delivered: DeliverRequest[] = [];
  const delivery: Delivery = {
    prepare: async () => {
      throw new Error('an exported image needs no destination prepared');
    },
    deliver: async (request) => {
      if (request.name === fail) throw new Error('host storage is full');
      delivered.push(request);
      return { how: 'downloaded', name: request.name, fellBack: null };
    },
  };
  return { delivery, delivered };
}

/** An engine that exports one marked byte per page; `failPage` makes that page's render throw. */
function exportEngine({ failPage }: { failPage?: number } = {}) {
  const calls: string[] = [];
  const engine = fakeEngine({
    parsePageSelection: async (input) => {
      if (input === '7') throw new EngineError('InvalidPageSelection', 'page 7 is past the last page (3)');
      return input === '2-3' ? [1, 2] : [0];
    },
    firstPageTooLargeToExport: async () => null,
    pageImageFileNames: async (name, pages, _total, format) =>
      pages.map((page) => `${name.replace('.pdf', '')}-${page + 1}.${format === 'png' ? 'png' : 'jpg'}`),
    openImageExport: async (handle) => (calls.push(`open:${handle}`), 40),
    exportPageImage: async (exportHandle, page, dpi, format) => {
      calls.push(`page:${exportHandle}:${page}:${dpi}:${format}`);
      if (page === failPage) throw new EngineError('RenderFailed', 'page could not be rendered');
      return new Uint8Array([page]);
    },
    closeImageExport: async (exportHandle) => (calls.push(`close:${exportHandle}`), true),
  });
  return { engine, calls };
}

function Harness({
  engine,
  delivery,
  document = openDocument(),
  settle,
  currentPage,
}: {
  engine: Engine;
  delivery: Delivery;
  document?: OpenDocument;
  settle?: () => Promise<unknown>;
  currentPage?: () => number;
}) {
  const session = useExportImages(engine, document, delivery, settle, currentPage);
  return <ExportImagesControls session={session} />;
}

const exportButton = () => screen.getByRole('button', { name: 'Export images' });
const dialog = () => screen.findByRole('dialog', { name: 'Export pages as images' });

describe('ExportImagesControls', () => {
  it('exports every page as PNG at 150 DPI by default, one file per page under the core names', async () => {
    const { engine, calls } = exportEngine();
    const { delivery, delivered } = recording();
    let settled = 0;
    render(<Harness engine={engine} delivery={delivery} settle={async () => void settled++} />);
    fireEvent.click(exportButton());

    const shown = await dialog();
    expect(within(shown).getByRole('radio', { name: 'All pages' })).toHaveProperty('checked', true);
    expect(within(shown).getByRole('radio', { name: 'PNG' })).toHaveProperty('checked', true);
    expect(within(shown).getByRole('textbox', { name: 'Resolution (72-400 DPI)' })).toHaveProperty('value', '150');

    fireEvent.click(within(shown).getByRole('button', { name: 'Export' }));
    expect(await screen.findByText('Exported 3 pages as PNG.')).toBeTruthy();
    expect(settled).toBe(1);
    expect(calls).toEqual(['open:7', 'page:40:0:150:png', 'page:40:1:150:png', 'page:40:2:150:png', 'close:40']);
    expect(delivered).toEqual(
      [0, 1, 2].map((page) => ({
        bytes: new Uint8Array([page]),
        name: `a-${page + 1}.png`,
        type: 'image/png',
        destination: { kind: 'newFile' },
      })),
    );
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('exports a typed range as JPEG at the typed resolution', async () => {
    const { engine, calls } = exportEngine();
    const { delivery, delivered } = recording();
    render(<Harness engine={engine} delivery={delivery} />);
    fireEvent.click(exportButton());
    const shown = await dialog();

    // Typing a range picks "Custom range" by itself.
    fireEvent.change(within(shown).getByRole('textbox', { name: 'Page range' }), { target: { value: '2-3' } });
    expect(within(shown).getByRole('radio', { name: 'Custom range' })).toHaveProperty('checked', true);
    fireEvent.click(within(shown).getByRole('radio', { name: 'JPEG' }));
    fireEvent.change(within(shown).getByRole('textbox', { name: 'Resolution (72-400 DPI)' }), { target: { value: '96' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Export' }));

    expect(await screen.findByText('Exported 2 pages as JPEG.')).toBeTruthy();
    expect(calls).toEqual(['open:7', 'page:40:1:96:jpeg', 'page:40:2:96:jpeg', 'close:40']);
    expect(delivered.map(({ name, type }) => [name, type])).toEqual([
      ['a-2.jpg', 'image/jpeg'],
      ['a-3.jpg', 'image/jpeg'],
    ]);
  });

  it('keeps only up to three digits in the resolution box, as the native boxes do', async () => {
    const { engine } = exportEngine();
    const { delivery } = recording();
    render(<Harness engine={engine} delivery={delivery} />);
    fireEvent.click(exportButton());
    const box = within(await dialog()).getByRole('textbox', { name: 'Resolution (72-400 DPI)' });
    fireEvent.change(box, { target: { value: '1a2.5' } });
    expect(box).toHaveProperty('value', '125');
    fireEvent.change(box, { target: { value: '6000' } });
    expect(box).toHaveProperty('value', '600');
  });

  it('exports the page the reader is on', async () => {
    const { engine, calls } = exportEngine();
    const { delivery } = recording();
    render(<Harness engine={engine} delivery={delivery} currentPage={() => 2} />);
    fireEvent.click(exportButton());
    const shown = await dialog();
    fireEvent.click(within(shown).getByRole('radio', { name: 'Current page' }));
    fireEvent.click(within(shown).getByRole('button', { name: 'Export' }));
    expect(await screen.findByText('Exported 1 page as PNG.')).toBeTruthy();
    expect(calls).toEqual(['open:7', 'page:40:2:150:png', 'close:40']);
  });

  it('keeps the dialog open with the core\'s sentence for a range it cannot read, and exports nothing', async () => {
    const { engine, calls } = exportEngine();
    const { delivery, delivered } = recording();
    render(<Harness engine={engine} delivery={delivery} />);
    fireEvent.click(exportButton());
    const shown = await dialog();
    fireEvent.change(within(shown).getByRole('textbox', { name: 'Page range' }), { target: { value: '7' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Export' }));

    expect((await within(shown).findByRole('alert')).textContent).toBe('Page 7 is past the last page (3).');
    expect(screen.getByRole('dialog', { name: 'Export pages as images' })).toBeTruthy();
    expect(calls).toEqual([]);
    expect(delivered).toEqual([]);

    // Changing a choice clears the message.
    fireEvent.change(within(shown).getByRole('textbox', { name: 'Page range' }), { target: { value: '2-3' } });
    expect(within(shown).queryByRole('alert')).toBeNull();
  });

  it('says a document that withholds extraction cannot be exported, with only Cancel to press', async () => {
    const { engine } = exportEngine();
    const { delivery } = recording();
    render(<Harness engine={engine} delivery={delivery} document={openDocument(false)} />);
    fireEvent.click(exportButton());
    const shown = await dialog();
    expect(within(shown).getByRole('alert').textContent).toBe(
      'This document does not permit extracting its pages as images.',
    );
    expect(within(shown).getByRole('button', { name: 'Export' })).toHaveProperty('disabled', true);
    for (const radio of within(shown).getAllByRole('radio')) expect(radio).toHaveProperty('disabled', true);
    fireEvent.click(within(shown).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(document.activeElement).toBe(exportButton());
  });

  it('stops at the first page that fails, counts what was written and closes the export', async () => {
    const { engine, calls } = exportEngine({ failPage: 1 });
    const { delivery, delivered } = recording();
    render(<Harness engine={engine} delivery={delivery} />);
    fireEvent.click(exportButton());
    fireEvent.click(within(await dialog()).getByRole('button', { name: 'Export' }));
    expect(await screen.findByText('Page 2 could not be exported: Page could not be rendered. 1 written.')).toBeTruthy();
    expect(calls).toEqual(['open:7', 'page:40:0:150:png', 'page:40:1:150:png', 'close:40']);
    expect(delivered.map(({ name }) => name)).toEqual(['a-1.png']);
  });

  it('stops at a file the host refused to take', async () => {
    const { engine, calls } = exportEngine();
    const { delivery } = recording('a-2.png');
    render(<Harness engine={engine} delivery={delivery} />);
    fireEvent.click(exportButton());
    fireEvent.click(within(await dialog()).getByRole('button', { name: 'Export' }));
    expect(await screen.findByText('Could not write a-2.png. 1 written.')).toBeTruthy();
    expect(calls.at(-1)).toBe('close:40');
  });

  it('says nothing was written when the snapshot could not be taken', async () => {
    const { engine } = exportEngine();
    engine.openImageExport = async () => {
      throw new EngineError('SaveFailed', 'the document could not be written');
    };
    const { delivery, delivered } = recording();
    render(<Harness engine={engine} delivery={delivery} />);
    fireEvent.click(exportButton());
    fireEvent.click(within(await dialog()).getByRole('button', { name: 'Export' }));
    expect(await screen.findByText('Could not export: The document could not be written. No file was written.')).toBeTruthy();
    expect(delivered).toEqual([]);
  });
});
