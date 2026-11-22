import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { createRef } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DocumentInfo, Engine, RenderedPage } from '@vitela/core';
import { fakeEngine } from '../test/fakeEngine';
import { VitelaViewer, type VitelaViewerHandle } from './VitelaViewer';

const created = vi.hoisted(() => ({ options: [] as unknown[], terminated: 0 }));
vi.mock('@vitela/core', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@vitela/core')>();
  return {
    ...actual,
    createEngine: (options: unknown) => {
      created.options.push(options);
      return Object.assign(fakeEngine({ open: async () => ({ handle: 1, info: info(3) }), renderPage: async () => page }), {
        ready: Promise.resolve(),
        terminate: () => void created.terminated++,
      });
    },
  };
});

function info(pageCount: number): DocumentInfo {
  return {
    pageCount,
    pages: Array(pageCount).fill({ widthPt: 612, heightPt: 792, rotationDegrees: 0 }),
    pageIds: Array.from({ length: pageCount }, (_, index) => index),
    textExtractionAllowed: true,
    documentAssemblyAllowed: true,
    blocks: [{ source: { kind: 'base' }, part: null, start: 0, count: pageCount }],
    importRefusal: null,
    annotationEditingAllowed: true,
  };
}
const page: RenderedPage = { width: 1, height: 1, rgba: new Uint8ClampedArray(4) };

/** An engine that opens every document with two pages, recording the bytes it was given. */
function openingEngine(overrides: Partial<Engine> = {}) {
  const opened: Uint8Array[] = [];
  const engine = fakeEngine({
    open: async (bytes) => (opened.push(bytes), { handle: opened.length, info: info(2) }),
    renderPage: async () => page,
    ...overrides,
  });
  return Object.assign(engine, { opened });
}

const settle = () =>
  act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });

beforeEach(() => {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
    putImageData: () => {},
  } as unknown as CanvasRenderingContext2D);
  vi.stubGlobal('ImageData', class {});
  created.options.length = 0;
  created.terminated = 0;
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('VitelaViewer', () => {
  it('opens bytes given as its source, under the name given, without taking the caller’s buffer', async () => {
    const engine = openingEngine();
    const bytes = new Uint8Array([37, 80, 68, 70]);
    render(<VitelaViewer engine={engine} source={bytes} name="report.pdf" />);
    expect(await screen.findByText('report.pdf — 2 pages')).toBeTruthy();
    expect([...engine.opened[0]!]).toEqual([37, 80, 68, 70]);
    expect(engine.opened[0]).not.toBe(bytes);
    expect(bytes.byteLength).toBe(4);
  });

  it('names bytes "document.pdf" when no name is given', async () => {
    render(<VitelaViewer engine={openingEngine()} source={new Uint8Array([37])} />);
    expect(await screen.findByText('document.pdf — 2 pages')).toBeTruthy();
  });

  it('opens a File as it is, under its own name', async () => {
    render(<VitelaViewer engine={openingEngine()} source={new File([new Uint8Array([37])], 'picked.pdf')} />);
    expect(await screen.findByText('picked.pdf — 2 pages')).toBeTruthy();
  });

  it('fetches a URL source and names it after the last part of its path', async () => {
    const fetched: string[] = [];
    vi.stubGlobal('fetch', async (url: string) => (fetched.push(String(url)), new Response(new Uint8Array([37, 80]))));
    const engine = openingEngine();
    render(<VitelaViewer engine={engine} source="https://example.test/files/annual%20report.pdf?v=2" />);
    expect(await screen.findByText('annual report.pdf — 2 pages')).toBeTruthy();
    expect(fetched).toEqual(['https://example.test/files/annual%20report.pdf?v=2']);
    expect([...engine.opened[0]!]).toEqual([37, 80]);
  });

  it('says why a URL could not be opened', async () => {
    vi.stubGlobal('fetch', async () => new Response('missing', { status: 404, statusText: 'Not Found' }));
    render(<VitelaViewer engine={openingEngine()} source="/files/gone.pdf" />);
    expect(await screen.findByText('Could not open gone.pdf: HTTP 404 Not Found')).toBeTruthy();
  });

  it('opens the new source when the source changes', async () => {
    const engine = openingEngine();
    const { rerender } = render(<VitelaViewer engine={engine} source={new Uint8Array([1])} name="one.pdf" />);
    expect(await screen.findByText('one.pdf — 2 pages')).toBeTruthy();
    rerender(<VitelaViewer engine={engine} source={new Uint8Array([2])} name="two.pdf" />);
    expect(await screen.findByText('two.pdf — 2 pages')).toBeTruthy();
    expect(engine.opened).toHaveLength(2);
  });

  it('waits for the engine to load before opening, and says it is loading', async () => {
    let loaded!: () => void;
    const engine = Object.assign(openingEngine(), { ready: new Promise<void>((resolve) => (loaded = resolve)) });
    render(<VitelaViewer engine={engine} source={new Uint8Array([1])} name="a.pdf" />);
    expect(screen.getByText('Loading the PDF engine…')).toBeTruthy();
    await settle();
    expect(engine.opened).toHaveLength(0);
    act(() => loaded());
    expect(await screen.findByText('a.pdf — 2 pages')).toBeTruthy();
  });

  it('says when the engine failed to load', async () => {
    const engine = Object.assign(openingEngine(), { ready: Promise.reject(new Error('wasm blocked')) });
    render(<VitelaViewer engine={engine} />);
    expect(await screen.findByText('The PDF engine failed to load: wasm blocked')).toBeTruthy();
  });

  it('hands the saved bytes and the document name to onSave', async () => {
    const saved = new Uint8Array([7, 7, 7]);
    const onSave = vi.fn();
    const engine = openingEngine({ saveToBytes: async () => saved });
    render(<VitelaViewer engine={engine} source={new Uint8Array([1])} name="a.pdf" onSave={onSave} />);
    await screen.findByText('a.pdf — 2 pages');

    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByText('Saved a.pdf.')).toBeTruthy();
    expect(onSave).toHaveBeenCalledExactlyOnceWith(saved, 'a.pdf');
  });

  it('shows the file picker by default, and none when told not to', async () => {
    const { unmount } = render(<VitelaViewer engine={openingEngine()} />);
    expect(await screen.findByLabelText('Open PDF')).toBeTruthy();
    unmount();
    render(<VitelaViewer engine={openingEngine()} filePicker={false} source={new Uint8Array([1])} />);
    await screen.findByText('document.pdf — 2 pages');
    expect(screen.queryByLabelText('Open PDF')).toBeNull();
  });

  it('opens documents in the initial screen it is given', async () => {
    render(<VitelaViewer engine={openingEngine()} source={new Uint8Array([1])} initialScreen="organize" />);
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Organize' }).getAttribute('aria-pressed')).toBe('true'),
    );
  });

  it('takes only the dropped files that land on it, by default', async () => {
    const outside = document.body.appendChild(document.createElement('div'));
    const engine = openingEngine();
    render(<VitelaViewer engine={engine} />);
    await screen.findByLabelText('Open PDF');
    const pdf = new File([new Uint8Array([37])], 'dropped.pdf', { type: 'application/pdf' });
    for (const type of ['dragenter', 'dragover', 'drop']) {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, 'dataTransfer', { value: { types: ['Files'], files: [pdf], dropEffect: 'none' } });
      fireEvent(outside, event);
    }
    await settle();
    expect(engine.opened).toHaveLength(0);
    outside.remove();
  });

  it('opens and saves through its handle', async () => {
    const onSave = vi.fn();
    const viewer = createRef<VitelaViewerHandle>();
    const engine = openingEngine({ saveToBytes: async () => new Uint8Array([5]) });
    render(<VitelaViewer engine={engine} onSave={onSave} ref={viewer} />);

    act(() => viewer.current!.open(new Uint8Array([1]), 'handed.pdf'));
    expect(await screen.findByText('handed.pdf — 2 pages')).toBeTruthy();
    act(() => viewer.current!.save());
    await waitFor(() => expect(onSave).toHaveBeenCalledExactlyOnceWith(new Uint8Array([5]), 'handed.pdf'));
  });

  describe('its own engine', () => {
    it('starts one with the engine options when given no engine, and stops it on unmount', async () => {
      const { unmount } = render(
        <VitelaViewer engineOptions={{ assetBaseUrl: 'https://cdn.example.test/vitela/' }} source={new Uint8Array([1])} />,
      );
      expect(await screen.findByText('document.pdf — 3 pages')).toBeTruthy();
      expect(created.options).toEqual([{ assetBaseUrl: 'https://cdn.example.test/vitela/' }]);
      unmount();
      expect(created.terminated).toBe(1);
    });

    it('never starts nor stops one when the host gives its engine', () => {
      const engine = Object.assign(openingEngine(), { terminate: vi.fn() });
      const { unmount } = render(<VitelaViewer engine={engine} />);
      unmount();
      expect(created.options).toEqual([]);
      expect(engine.terminate).not.toHaveBeenCalled();
    });
  });
});
