import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useCallback, useRef } from 'react';
import { EngineError, type Engine } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { EditorScope } from '../document/editorScope';
import type { OpenDocument } from '../document/useDocument';
import type { PrintJob, PrintSurface } from './printFrame';
import { printDpi, type PrintSheet } from './printSheets';
import { PrintControls } from './PrintControls';
import { usePrint, printBusy, type PrintHost } from './usePrint';
import { usePrintKey } from './usePrintKey';

const document: OpenDocument = {
  name: 'report.pdf',
  handle: 7,
  generation: 1,
  info: {
    pageCount: 2,
    pages: [
      { widthPt: 612, heightPt: 792, rotationDegrees: 0 },
      { widthPt: 612, heightPt: 792, rotationDegrees: 90 },
    ],
    pageIds: [0, 1],
    textExtractionAllowed: false,
    documentAssemblyAllowed: true,
    blocks: [],
    importRefusal: null,
    annotationEditingAllowed: true,
    formFillAllowed: true,
  },
};

/** A PNG header the size of a page at 150 DPI: Letter upright, or turned a quarter. */
function png(width: number, height: number, marker: number): Uint8Array {
  const bytes = new Uint8Array(25);
  bytes.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52]);
  new DataView(bytes.buffer).setUint32(16, width);
  new DataView(bytes.buffer).setUint32(20, height);
  bytes[24] = marker;
  return bytes;
}

let created: string[];
let revoked: string[];
beforeEach(() => {
  created = [];
  revoked = [];
  let next = 0;
  class TestURL extends URL {
    static override createObjectURL = () => {
      const url = `blob:print/${next++}`;
      created.push(url);
      return url;
    };
    static override revokeObjectURL = (url: string) => void revoked.push(url);
  }
  vi.stubGlobal('URL', TestURL);
});
afterEach(cleanup);

interface EngineOptions {
  readonly pageCount?: number;
  readonly oversized?: number | null;
  /** The page whose drawing fails. */
  readonly failing?: number;
  /** Holds each page until the test lets it go. */
  readonly gate?: () => Promise<void>;
}

function printEngine({ pageCount = 2, oversized = null, failing, gate }: EngineOptions = {}) {
  const log: string[] = [];
  const engine: Engine = fakeEngine({
    openPrintSnapshot: async (handle) => (log.push(`open:${handle}`), { handle: 40, pageCount }),
    firstPageTooLargeToExport: async (handle, pages, dpi) => (log.push(`ceiling:${handle}:${pages.join(',')}:${dpi}`), oversized),
    exportPageImage: async (snapshot, page, dpi, format) => {
      log.push(`draw:${snapshot}:${page}:${dpi}:${format}`);
      await gate?.();
      if (page === failing) throw new EngineError('RenderFailed', 'the page could not be rendered');
      return page === 1 ? png(1650, 1275, page) : png(1275, 1650, page);
    },
    closeImageExport: async (snapshot) => (log.push(`close:${snapshot}`), true),
  });
  return { engine, log };
}

function fakeSurface(log: string[], frameGate?: () => Promise<void>) {
  const jobs: { sheets: readonly PrintSheet[]; title: string; host: HTMLElement; closed: () => void; removed: boolean }[] = [];
  const surface: PrintSurface = async (host, title, sheets) => {
    log.push('frame');
    await frameGate?.();
    let closed = () => {};
    const entry = { sheets, title, host, closed: () => closed(), removed: false };
    jobs.push(entry);
    const job: PrintJob = {
      print: () => void log.push('print'),
      afterPrint: (listener) => void (closed = listener),
      remove: () => void ((entry.removed = true), log.push('remove')),
    };
    return job;
  };
  return { surface, jobs };
}

/** The workspace's wiring: Print in the Tools menu (a stand-in button here), its status in the bar, its key on the editor. */
function Harness({ engine, host, scope }: { engine: Engine; host: Omit<PrintHost, 'frameHost'>; scope?: EditorScope }) {
  const root = useRef<HTMLDivElement>(null);
  const frameHost = useCallback(() => root.current, []);
  const session = usePrint(engine, document, { ...host, frameHost });
  usePrintKey(() => void session.print(), scope);
  const busy = printBusy(session.activity);
  return (
    <div ref={root}>
      <button type="button" aria-disabled={busy} onClick={busy ? undefined : () => void session.print()}>
        Print
      </button>
      <PrintControls session={session} />
    </div>
  );
}

const printButton = () => screen.getByRole('button', { name: 'Print' });

describe('Print', () => {
  it('prints what a save would write: every page of the snapshot at 150 DPI, in order, after the queued edits', async () => {
    const { engine, log } = printEngine();
    const { surface, jobs } = fakeSurface(log);
    const settleEdits = async () => void log.push('settled');
    render(<Harness engine={engine} host={{ settleEdits, surface }} />);
    fireEvent.click(printButton());
    await waitFor(() => expect(log).toContain('print'));
    // The snapshot is closed before the dialog opens; a document that withholds extraction still prints.
    expect(log).toEqual([
      'settled',
      'open:7',
      'ceiling:7:0,1:150',
      'draw:40:0:150:png',
      'draw:40:1:150:png',
      'close:40',
      'frame',
      'print',
    ]);
    const [job] = jobs;
    expect(job!.title).toBe('report');
    expect(job!.host.contains(printButton())).toBe(true);
    // The turned page prints on a landscape sheet.
    expect(job!.sheets).toEqual([
      { url: 'blob:print/0', widthPt: 612, heightPt: 792 },
      { url: 'blob:print/1', widthPt: 792, heightPt: 612 },
    ]);
    expect(screen.queryByRole('status')).toBeNull();
    expect(revoked).toEqual([]);

    // The dialog closed: the frame and its images go.
    act(() => job!.closed());
    expect(job!.removed).toBe(true);
    expect(revoked).toEqual(['blob:print/0', 'blob:print/1']);
  });

  it('shows the page being drawn and stops after it on Cancel print, freeing the snapshot and the images', async () => {
    const releases: (() => void)[] = [];
    const gate = () => new Promise<void>((resolve) => releases.push(resolve));
    const { engine, log } = printEngine({ pageCount: 3, gate });
    const { surface } = fakeSurface(log);
    render(<Harness engine={engine} host={{ settleEdits: async () => {}, surface }} />);
    fireEvent.click(printButton());
    expect(await screen.findByText('Preparing page 1 of 3 for printing…')).toBeTruthy();
    expect(printButton().getAttribute('aria-disabled')).toBe('true');
    fireEvent.click(screen.getByRole('button', { name: 'Cancel print' }));
    await act(async () => releases.shift()!());
    expect(await screen.findByText('Print cancelled.')).toBeTruthy();
    expect(log.filter((line) => line.startsWith('draw'))).toEqual(['draw:40:0:150:png']);
    expect(log).toContain('close:40');
    expect(log).not.toContain('frame');
    expect(revoked).toEqual(created);
    expect(screen.queryByRole('button', { name: 'Cancel print' })).toBeNull();
    expect(printButton().getAttribute('aria-disabled')).toBe('false');
  });

  it('hears Cancel print pressed while the last page is drawn: no frame, no dialog, nothing kept', async () => {
    const releases: (() => void)[] = [];
    const gate = () => new Promise<void>((resolve) => releases.push(resolve));
    const { engine, log } = printEngine({ pageCount: 1, gate });
    const { surface } = fakeSurface(log);
    render(<Harness engine={engine} host={{ settleEdits: async () => {}, surface }} />);
    fireEvent.click(printButton());
    expect(await screen.findByText('Preparing page 1 of 1 for printing…')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel print' }));
    await act(async () => releases.shift()!());
    expect(await screen.findByText('Print cancelled.')).toBeTruthy();
    expect(log).toContain('close:40');
    expect(log).not.toContain('frame');
    expect(log).not.toContain('print');
    expect(created).toHaveLength(1);
    expect(revoked).toEqual(created);
    expect(printButton().getAttribute('aria-disabled')).toBe('false');
  });

  it('hears Cancel print pressed while the frame gets its pages ready: the frame goes, the dialog never opens', async () => {
    const releases: (() => void)[] = [];
    const frameGate = () => new Promise<void>((resolve) => releases.push(resolve));
    const { engine, log } = printEngine();
    const { surface, jobs } = fakeSurface(log, frameGate);
    render(<Harness engine={engine} host={{ settleEdits: async () => {}, surface }} />);
    fireEvent.click(printButton());
    await waitFor(() => expect(releases).toHaveLength(1));
    fireEvent.click(screen.getByRole('button', { name: 'Cancel print' }));
    await act(async () => releases.shift()!());
    expect(await screen.findByText('Print cancelled.')).toBeTruthy();
    expect(log).not.toContain('print');
    expect(jobs[0]!.removed).toBe(true);
    expect(revoked).toEqual(created);
    expect(printButton().getAttribute('aria-disabled')).toBe('false');
  });

  it('draws a long document at a lower resolution, so all its pages fit the print budget', async () => {
    const { engine, log } = printEngine({ pageCount: 100 });
    const { surface, jobs } = fakeSurface(log);
    render(<Harness engine={engine} host={{ settleEdits: async () => {}, surface }} />);
    fireEvent.click(printButton());
    await waitFor(() => expect(log).toContain('print'));
    const dpi = printDpi(document.info.pages, 100);
    expect(dpi).toBeLessThan(150);
    expect(log).toContain(`ceiling:7:${Array.from({ length: 100 }, (_, i) => i).join(',')}:${dpi}`);
    expect(log.filter((line) => line.startsWith('draw'))).toHaveLength(100);
    expect(log.every((line) => !line.startsWith('draw') || line.endsWith(`:${dpi}:png`))).toBe(true);
    // The sheet is still the page's size: the fewer pixels are spread over it.
    expect(jobs[0]!.sheets[0]!.widthPt).toBeCloseTo((1275 * 72) / dpi, 1);
  });

  it('refuses a page over the raster ceiling before drawing any', async () => {
    const { engine, log } = printEngine({ oversized: 1 });
    const { surface } = fakeSurface(log);
    render(<Harness engine={engine} host={{ settleEdits: async () => {}, surface }} />);
    fireEvent.click(printButton());
    expect((await screen.findByRole('alert')).textContent).toBe('Page 2 is too large to print.');
    expect(log.some((line) => line.startsWith('draw'))).toBe(false);
    expect(log).toContain('close:40');
  });

  it('names the page that could not be drawn, and lets the images already drawn go', async () => {
    const { engine, log } = printEngine({ failing: 1 });
    const { surface } = fakeSurface(log);
    render(<Harness engine={engine} host={{ settleEdits: async () => {}, surface }} />);
    fireEvent.click(printButton());
    expect((await screen.findByRole('alert')).textContent).toBe(
      'Could not prepare page 2 for printing: The page could not be rendered.',
    );
    expect(log).toContain('close:40');
    expect(log).not.toContain('frame');
    expect(revoked).toEqual(['blob:print/0']);
    expect(printButton().getAttribute('aria-disabled')).toBe('false');
  });

  it("says so when the snapshot cannot be opened, in the core's words", async () => {
    const engine = fakeEngine({
      openPrintSnapshot: async () => {
        throw new EngineError('SaveFailed', 'the document could not be written');
      },
    });
    render(<Harness engine={engine} host={{ settleEdits: async () => {}, surface: fakeSurface([]).surface }} />);
    fireEvent.click(printButton());
    expect((await screen.findByRole('alert')).textContent).toBe('Could not print: The document could not be written.');
  });

  it('takes the last frame away when a new print starts before its dialog reported closing', async () => {
    const { engine, log } = printEngine({ pageCount: 1 });
    const { surface, jobs } = fakeSurface(log);
    render(<Harness engine={engine} host={{ settleEdits: async () => {}, surface }} />);
    fireEvent.click(printButton());
    await waitFor(() => expect(jobs).toHaveLength(1));
    await waitFor(() => expect(printButton().getAttribute('aria-disabled')).toBe('false'));
    fireEvent.click(printButton());
    await waitFor(() => expect(jobs).toHaveLength(2));
    expect(jobs[0]!.removed).toBe(true);
    expect(revoked).toEqual(['blob:print/0']);
    // The first dialog's late close does not take the second frame.
    act(() => jobs[0]!.closed());
    expect(jobs[1]!.removed).toBe(false);
    cleanup();
    expect(jobs[1]!.removed).toBe(true);
    expect(revoked).toEqual(['blob:print/0', 'blob:print/1']);
  });

  it('builds no frame for an editor that went away while the pages were drawn', async () => {
    const releases: (() => void)[] = [];
    const gate = () => new Promise<void>((resolve) => releases.push(resolve));
    const { engine, log } = printEngine({ pageCount: 1, gate });
    const { surface } = fakeSurface(log);
    render(<Harness engine={engine} host={{ settleEdits: async () => {}, surface }} />);
    fireEvent.click(printButton());
    await waitFor(() => expect(releases).toHaveLength(1));
    cleanup();
    await act(async () => releases.shift()!());
    await waitFor(() => expect(log).toContain('close:40'));
    expect(log).not.toContain('frame');
    expect(revoked).toEqual(created);
  });
});

describe('the print key', () => {
  const scope = (owns: boolean, modal = false): EditorScope => ({
    ownsEvent: () => owns,
    takesDrop: () => owns,
    modalOpen: () => modal,
  });

  async function pressed(editorScope: EditorScope) {
    const { engine, log } = printEngine({ pageCount: 1 });
    const { surface } = fakeSurface(log);
    render(<Harness engine={engine} host={{ settleEdits: async () => {}, surface }} scope={editorScope} />);
    const event = new KeyboardEvent('keydown', { key: 'p', ctrlKey: true, bubbles: true, cancelable: true });
    printButton().dispatchEvent(event);
    await act(async () => {});
    return { log, prevented: event.defaultPrevented };
  }

  it("prints the editor's pages, instead of the browser printing the page, when the key is the editor's", async () => {
    const { log, prevented } = await pressed(scope(true));
    expect(prevented).toBe(true);
    await waitFor(() => expect(log).toContain('print'));
  });

  it('leaves the key to the browser when it is aimed elsewhere on the page, or a dialog of the editor is open', async () => {
    for (const editorScope of [scope(false), scope(true, true)]) {
      const { log, prevented } = await pressed(editorScope);
      expect(prevented).toBe(false);
      expect(log).toEqual([]);
      cleanup();
    }
  });
});
