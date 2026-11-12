import { describe, expect, it } from 'vitest';
import { handleRequest } from './handler';
import type { Engine } from '../core/types';

const page = { width: 2, height: 1, rgba: new Uint8ClampedArray(8) };

function fakeEngine(overrides: Partial<Engine> = {}): Engine {
  return {
    open: async () => ({ pageCount: 4 }),
    renderPage: async () => page,
    ...overrides,
  };
}

describe('handleRequest', () => {
  it('answers open with the document info and transfers nothing', async () => {
    const reply = await handleRequest(fakeEngine(), {
      id: 7,
      request: { kind: 'open', bytes: new Uint8Array([1]) },
    });
    expect(reply).toEqual({ message: { type: 'result', id: 7, value: { pageCount: 4 } }, transfer: [] });
  });

  it('answers renderPage and transfers the pixel buffer instead of copying it', async () => {
    const calls: [number, number][] = [];
    const engine = fakeEngine({
      renderPage: async (index, dpi) => {
        calls.push([index, dpi]);
        return page;
      },
    });
    const reply = await handleRequest(engine, {
      id: 8,
      request: { kind: 'renderPage', pageIndex: 2, dpi: 96 },
    });
    expect(calls).toEqual([[2, 96]]);
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
      message: { type: 'error', id: 9, message: 'open: PdfiumLibraryInternalError(FormatError)' },
      transfer: [],
    });
  });
});
