import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { DocumentInfo, Engine } from '../../core/types';
import { useDocument } from './useDocument';

function file(name: string, bytes = [37, 80, 68, 70]): File {
  return new File([new Uint8Array(bytes)], name, { type: 'application/pdf' });
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function engineWith(open: Engine['open']): Engine {
  return { open, renderPage: () => Promise.reject(new Error('unused')) };
}

describe('useDocument', () => {
  it('starts with no document', () => {
    const { result } = renderHook(() => useDocument(engineWith(() => new Promise(() => {}))));
    expect(result.current.state).toEqual({ status: 'empty' });
  });

  it('passes the file bytes to the engine and reports the opened document', async () => {
    const seen: number[][] = [];
    const engine = engineWith(async (bytes) => {
      seen.push([...bytes]);
      return { pageCount: 2 };
    });
    const { result } = renderHook(() => useDocument(engine));

    act(() => result.current.open(file('a.pdf', [1, 2, 3])));
    expect(result.current.state).toEqual({ status: 'opening', name: 'a.pdf' });

    await waitFor(() => expect(result.current.state.status).toBe('open'));
    expect(result.current.state).toEqual({ status: 'open', name: 'a.pdf', pageCount: 2, generation: 1 });
    expect(seen).toEqual([[1, 2, 3]]);
  });

  it('reports a failed open with the engine message', async () => {
    const engine = engineWith(() => Promise.reject(new Error('open: FormatError')));
    const { result } = renderHook(() => useDocument(engine));
    act(() => result.current.open(file('broken.pdf')));
    await waitFor(() =>
      expect(result.current.state).toEqual({ status: 'failed', name: 'broken.pdf', message: 'open: FormatError' }),
    );
  });

  it('ignores a slower earlier open once a later one was started', async () => {
    const first = deferred<DocumentInfo>();
    const second = deferred<DocumentInfo>();
    const pending = [first, second];
    const engine = engineWith(() => pending.shift()!.promise);
    const { result } = renderHook(() => useDocument(engine));

    act(() => result.current.open(file('first.pdf')));
    act(() => result.current.open(file('second.pdf')));
    await act(async () => second.resolve({ pageCount: 5 }));
    await act(async () => first.resolve({ pageCount: 1 }));

    expect(result.current.state).toMatchObject({ status: 'open', name: 'second.pdf', pageCount: 5 });
  });
});
