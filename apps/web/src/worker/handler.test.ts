import { describe, expect, it } from 'vitest';
import { handleRequest } from './handler';
import type { Engine } from '../core/types';

const page = { width: 2, height: 1, rgba: new Uint8ClampedArray(8) };
const info = { pageCount: 1, pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }], textExtractionAllowed: true };
const opened = { handle: 5, info };

function fakeEngine(overrides: Partial<Engine> = {}): Engine {
  return {
    open: async () => opened,
    renderPage: async () => page,
    close: async () => {},
    search: async () => [],
    selectText: async () => ({ text: '', rects: new Float32Array(0) }),
    selectWord: async () => ({ text: '', rects: new Float32Array(0) }),
    ...overrides,
  };
}

describe('handleRequest', () => {
  it('answers open with the handle and document info and transfers nothing', async () => {
    const reply = await handleRequest(fakeEngine(), {
      id: 7,
      request: { kind: 'open', bytes: new Uint8Array([1]) },
    });
    expect(reply).toEqual({ message: { type: 'result', id: 7, value: opened }, transfer: [] });
  });

  it('answers renderPage and transfers the pixel buffer instead of copying it', async () => {
    const calls: [number, number, number][] = [];
    const engine = fakeEngine({
      renderPage: async (document, index, dpi) => {
        calls.push([document, index, dpi]);
        return page;
      },
    });
    const reply = await handleRequest(engine, {
      id: 8,
      request: { kind: 'renderPage', document: 5, pageIndex: 2, dpi: 96 },
    });
    expect(calls).toEqual([[5, 2, 96]]);
    expect(reply.message).toEqual({ type: 'result', id: 8, value: page });
    expect(reply.transfer).toEqual([page.rgba.buffer]);
  });

  it('turns an engine failure into an error reply for that request', async () => {
    const engine = fakeEngine({
      open: async () => {
        throw new Error('open: PdfiumLibraryInternalError(FormatError)');
      },
    });
    const reply = await handleRequest(engine, {
      id: 9,
      request: { kind: 'open', bytes: new Uint8Array() },
    });
    expect(reply).toEqual({
      message: { type: 'error', id: 9, name: 'Error', message: 'open: PdfiumLibraryInternalError(FormatError)' },
      transfer: [],
    });
  });

  it('passes the password of an open request to the engine', async () => {
    const seen: (string | undefined)[] = [];
    const engine = fakeEngine({
      open: async (_bytes, password) => {
        seen.push(password);
        return opened;
      },
    });
    await handleRequest(engine, { id: 1, request: { kind: 'open', bytes: new Uint8Array(), password: 'secret' } });
    await handleRequest(engine, { id: 2, request: { kind: 'open', bytes: new Uint8Array() } });
    expect(seen).toEqual(['secret', undefined]);
  });

  it('forwards the name of a typed engine error, not only its message', async () => {
    const failure = new Error('the password does not open this document');
    failure.name = 'WrongPassword';
    const engine = fakeEngine({
      open: async () => {
        throw failure;
      },
    });
    const reply = await handleRequest(engine, { id: 3, request: { kind: 'open', bytes: new Uint8Array() } });
    expect(reply.message).toEqual({
      type: 'error',
      id: 3,
      name: 'WrongPassword',
      message: 'the password does not open this document',
    });
  });

  it('names a thrown non-Error value generically', async () => {
    const engine = fakeEngine({
      renderPage: async () => {
        throw 'pdfium aborted';
      },
    });
    const reply = await handleRequest(engine, { id: 4, request: { kind: 'renderPage', document: 5, pageIndex: 0, dpi: 72 } });
    expect(reply.message).toEqual({ type: 'error', id: 4, name: 'Error', message: 'pdfium aborted' });
  });

  it('closes the document a close request names', async () => {
    const closed: number[] = [];
    const engine = fakeEngine({
      close: async (document) => {
        closed.push(document);
      },
    });
    const reply = await handleRequest(engine, { id: 6, request: { kind: 'close', document: 5 } });
    expect(closed).toEqual([5]);
    expect(reply).toEqual({ message: { type: 'result', id: 6, value: undefined }, transfer: [] });
  });

  it('answers the text requests with what the engine found for that document and page', async () => {
    const hits = [{ pageIndex: 1, rects: new Float32Array([1, 2, 3, 4]) }];
    const selection = { text: 'beta', rects: new Float32Array([5, 6, 7, 8]) };
    const calls: unknown[][] = [];
    const engine = fakeEngine({
      search: async (...args) => (calls.push(['search', ...args]), hits),
      selectText: async (...args) => (calls.push(['selectText', ...args]), selection),
      selectWord: async (...args) => (calls.push(['selectWord', ...args]), selection),
    });
    const a = { x: 1, y: 2 };
    const b = { x: 3, y: 4 };
    expect((await handleRequest(engine, { id: 1, request: { kind: 'search', document: 5, query: 'beta' } })).message).toEqual({
      type: 'result',
      id: 1,
      value: hits,
    });
    const selected = await handleRequest(engine, {
      id: 2,
      request: { kind: 'selectText', document: 5, pageIndex: 1, anchor: a, focus: b },
    });
    expect(selected.message).toEqual({ type: 'result', id: 2, value: selection });
    await handleRequest(engine, { id: 3, request: { kind: 'selectWord', document: 5, pageIndex: 2, point: a } });
    expect(calls).toEqual([
      ['search', 5, 'beta'],
      ['selectText', 5, 1, a, b],
      ['selectWord', 5, 2, a],
    ]);
  });

  it('forwards TextExtractionDenied by name, like the password failures', async () => {
    const failure = new Error('text extraction is not permitted');
    failure.name = 'TextExtractionDenied';
    const engine = fakeEngine({
      search: async () => {
        throw failure;
      },
    });
    const reply = await handleRequest(engine, { id: 7, request: { kind: 'search', document: 5, query: 'x' } });
    expect(reply.message).toMatchObject({ type: 'error', id: 7, name: 'TextExtractionDenied' });
  });
});
