import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DocumentInfo, EngineClient, RenderedPage } from '@vitela/core';
import { fakeEngine } from '../test/fakeEngine';
import type { LaunchedFile } from '../features/launch/launchQueue';
import { App } from './App';

beforeEach(() => {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
    putImageData: () => {},
  } as unknown as CanvasRenderingContext2D);
  vi.stubGlobal('ImageData', class {});
  HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
    this.removeAttribute('open');
  };
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const info: DocumentInfo = {
  pageCount: 1,
  pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }],
  pageIds: [0],
  textExtractionAllowed: true,
  documentAssemblyAllowed: true,
  blocks: [{ source: { kind: 'base' }, part: null, start: 0, count: 1 }],
  importRefusal: null,
  annotationEditingAllowed: true,
  formFillAllowed: true,
};
const page: RenderedPage = { width: 1, height: 1, rgba: new Uint8ClampedArray(4) };

/** An engine that opens any PDF and takes a Document properties edit, so a test can make the document dirty. */
function editingEngine(): EngineClient {
  return {
    ...fakeEngine({
      open: async () => ({ handle: 1, info }),
      renderPage: async () => page,
      documentProperties: async () => ({ title: null, author: null, subject: null, keywords: null, creator: null, producer: null }),
      metadataRefusal: async () => null,
      applyEdit: async () => ({ info, annotations: [], addedAnnotationId: null, formFields: [], canUndo: true, canRedo: false }),
    }),
    ready: Promise.resolve(),
  } as unknown as EngineClient;
}

type Queries = ReturnType<typeof render>;

/** Opens a PDF through the page's Open action, then edits its title: the document now has unsaved edits. */
async function openAndEdit({ getByLabelText, getByRole, findByRole, findByText }: Queries) {
  const pdf = new File([new Uint8Array([37, 80, 68, 70])], 'edited.pdf', { type: 'application/pdf' });
  fireEvent.change(await waitFor(() => getByLabelText('Open PDF')), { target: { files: [pdf] } });
  fireEvent.click(await waitFor(() => getByRole('button', { name: 'Tools' })));
  fireEvent.click(getByRole('menuitem', { name: 'Document properties' }));
  const dialog = await findByRole('dialog', { name: 'Document properties' });
  fireEvent.change(getByLabelText('Title'), { target: { value: 'Annual report' } });
  fireEvent.click(dialog.querySelector('button[type="submit"]')!);
  await findByText('Unsaved changes.');
}

function loadingEngine() {
  let finish!: () => void;
  const ready = new Promise<void>((resolve) => (finish = resolve));
  const engine = { ...fakeEngine(), ready } as unknown as EngineClient;
  return { engine, finish };
}

describe('App', () => {
  // A remount replays the landing's entrance animation, so the page jumps
  // twice on every load.
  it('keeps the home page mounted while the engine finishes loading', async () => {
    const { engine, finish } = loadingEngine();
    const { container, getByLabelText } = render(<App engine={engine} pathname="/" />);
    const hero = container.querySelector('.hero');
    expect(hero).not.toBeNull();

    await act(async () => finish());

    expect(getByLabelText('Open PDF')).toBeTruthy();
    expect(container.querySelector('.hero')).toBe(hero);
  });

  // The page is all editor: a PDF dropped anywhere on it opens, not only
  // one dropped on the editor (the default of an embedded editor).
  it('opens a PDF dropped anywhere on the page, the site header included', async () => {
    const opened: Uint8Array[] = [];
    const engine = {
      ...fakeEngine({
        open: (bytes) => {
          opened.push(bytes);
          return new Promise(() => {});
        },
      }),
      ready: Promise.resolve(),
    } as unknown as EngineClient;
    const { container } = render(<App engine={engine} pathname="/" />);
    await act(async () => {});

    const pdf = new File([new Uint8Array([37, 80, 68, 70])], 'dropped.pdf', { type: 'application/pdf' });
    const header = container.querySelector('.site-header')!;
    for (const type of ['dragenter', 'dragover', 'drop']) {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, 'dataTransfer', { value: { types: ['Files'], files: [pdf], dropEffect: 'none' } });
      await act(async () => {
        header.dispatchEvent(event);
      });
    }
    await waitFor(() => expect(opened).toHaveLength(1));
  });

  describe('updates', () => {
    function fakeUpdates(ready: boolean) {
      const state = { ready, held: false, blocked: false };
      return {
        subscribe: () => () => {},
        state: () => state,
        hold: vi.fn(),
        apply: vi.fn(),
      };
    }
    const readyEngine = () => ({ ...fakeEngine(), ready: Promise.resolve() }) as unknown as EngineClient;

    it('shows no update banner while there is no new version', async () => {
      const { queryByRole } = render(<App engine={readyEngine()} pathname="/" updates={fakeUpdates(false)} />);
      await act(async () => {});
      expect(queryByRole('button', { name: 'Reload' })).toBeNull();
    });

    it('offers a new version and reloads into it on request', async () => {
      const updates = fakeUpdates(true);
      const { getByRole } = render(<App engine={readyEngine()} pathname="/" updates={updates} />);
      await act(async () => {});
      fireEvent.click(getByRole('button', { name: 'Reload' }));
      expect(updates.apply).toHaveBeenCalledTimes(1);
    });

    it('tells the updates whether the open document has unsaved edits', async () => {
      const updates = fakeUpdates(false);
      render(<App engine={readyEngine()} pathname="/" updates={updates} />);
      await act(async () => {});
      expect(updates.hold).toHaveBeenLastCalledWith(false);
    });
  });

  // T-267: the browser asks before a close or reload would lose the edits.
  it('tells the unload warning whether the open document has unsaved edits', async () => {
    const warnOnUnload = vi.fn();
    const queries = render(<App engine={editingEngine()} pathname="/" warnOnUnload={warnOnUnload} />);
    await act(async () => {});
    expect(warnOnUnload).toHaveBeenLastCalledWith(false);

    await openAndEdit(queries);
    expect(warnOnUnload).toHaveBeenLastCalledWith(true);
  });

  // Releasing the hold can reload into a new build at once: by then the
  // warning must be gone, or the reload itself asks "Leave site?".
  it('drops the unload warning before it lets a held update reload', async () => {
    const calls: string[] = [];
    const state = { ready: true, held: false, blocked: false };
    const updates = {
      subscribe: () => () => {},
      state: () => state,
      hold: (dirty: boolean) => void calls.push(`hold ${dirty}`),
      apply: () => {},
    };
    const warnOnUnload = (dirty: boolean) => void calls.push(`warn ${dirty}`);
    const queries = render(<App engine={editingEngine()} pathname="/" updates={updates} warnOnUnload={warnOnUnload} />);
    await act(async () => {});
    await openAndEdit(queries);
    calls.length = 0;

    // A new document starts clean.
    const pdf = new File([new Uint8Array([37, 80, 68, 70])], 'other.pdf', { type: 'application/pdf' });
    fireEvent.change(queries.getByLabelText('Open PDF'), { target: { files: [pdf] } });
    await waitFor(() => expect(calls).toEqual(['warn false', 'hold false']));
  });

  // T-266: a PDF the OS opened with the installed app.
  describe('launched files', () => {
    function fakeLaunches(launched: LaunchedFile | null) {
      let pending = launched;
      return {
        subscribe: () => () => {},
        pending: () => pending,
        take: vi.fn((file: LaunchedFile) => {
          if (pending !== file) return false;
          pending = null;
          return true;
        }),
      };
    }

    it('opens a launched PDF once the engine is ready, with its handle, and only once', async () => {
      const { engine, finish } = loadingEngine();
      const opened: Uint8Array[] = [];
      const recording = {
        ...engine,
        open: (bytes: Uint8Array) => {
          opened.push(bytes);
          return new Promise(() => {});
        },
      } as unknown as EngineClient;
      const file = new File([new Uint8Array([37, 80, 68, 70])], 'launched.pdf', { type: 'application/pdf' });
      const launches = fakeLaunches({ file });
      const { getByText } = render(<App engine={recording} pathname="/" launches={launches} />);
      // Not while the engine loads: the open action is not there yet.
      await act(async () => {});
      expect(launches.take).not.toHaveBeenCalled();

      await act(async () => finish());
      await waitFor(() => expect(opened).toHaveLength(1));
      expect(getByText('Opening launched.pdf…')).toBeTruthy();
      expect(launches.pending()).toBeNull();
    });

    // A launch whose files could not be read is not dropped silently.
    it('shows the usual open error for a launch it could not read', async () => {
      const unreadable = { name: 'gone.pdf', arrayBuffer: () => Promise.reject(new Error('the file is gone')) };
      const { findByText } = render(
        <App engine={editingEngine()} pathname="/" launches={fakeLaunches({ file: unreadable })} />,
      );
      expect(await findByText('Could not open gone.pdf: the file is gone')).toBeTruthy();
    });

    // The stamp is the file as the launch read it, so a change made while
    // the engine loaded is caught on Save, not written over.
    it('opens a launched file with the stamp it was read with, so Save notices a change made since', async () => {
      const file = new File([new Uint8Array([37, 80, 68, 70])], 'launched.pdf', { type: 'application/pdf', lastModified: 1000 });
      const handle = {
        kind: 'file',
        name: 'launched.pdf',
        getFile: async () => ({ lastModified: 2000, size: 4 }) as File,
        createWritable: async () => ({ write: async () => {}, close: async () => {} }),
        queryPermission: async () => 'granted' as PermissionState,
      };
      const launches = fakeLaunches({ file, handle, stamp: { lastModified: 1000, size: 4 } });
      const { findByText, getByRole, findByRole } = render(<App engine={editingEngine()} pathname="/" launches={launches} />);
      await findByText('launched.pdf — 1 page');
      fireEvent.click(getByRole('button', { name: 'Save' }));
      expect(await findByRole('dialog', { name: 'This file changed on disk since you opened it' })).toBeTruthy();
    });
  });
});
