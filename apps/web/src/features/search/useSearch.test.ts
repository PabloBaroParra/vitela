import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { EngineError } from '../../core/errors';
import type { SearchHit } from '../../core/types';
import { fakeEngine } from '../../test/fakeEngine';
import { currentHit } from './searchState';
import { useSearch } from './useSearch';

const hit = (pageIndex: number): SearchHit => ({ pageIndex, rects: new Float32Array([0, 0, 1, 1]) });

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

describe('useSearch', () => {
  it('searches the document it was given and starts at the first match', async () => {
    const calls: unknown[][] = [];
    const engine = fakeEngine({ search: async (...args) => (calls.push(args), [hit(2), hit(5)]) });
    const { result } = renderHook(() => useSearch(engine, 7, () => {}));

    act(() => result.current.submit('beta', false));
    expect(result.current.state).toEqual({ kind: 'searching', query: 'beta' });
    await waitFor(() => expect(result.current.state.kind).toBe('found'));
    expect(calls).toEqual([[7, 'beta']]);
    expect(currentHit(result.current.state)?.pageIndex).toBe(2);
  });

  it('moves on when the same query is submitted again, and back with Shift', async () => {
    const engine = fakeEngine({ search: async () => [hit(0), hit(1), hit(2)] });
    const { result } = renderHook(() => useSearch(engine, 1, () => {}));
    act(() => result.current.submit('beta', false));
    await waitFor(() => expect(result.current.state.kind).toBe('found'));

    act(() => result.current.submit('beta', false));
    expect(currentHit(result.current.state)?.pageIndex).toBe(1);
    act(() => result.current.submit('beta', true));
    act(() => result.current.submit('beta', true));
    expect(currentHit(result.current.state)?.pageIndex).toBe(2);
    act(() => result.current.next());
    expect(currentHit(result.current.state)?.pageIndex).toBe(0);
  });

  it('keeps only the answer to the latest query', async () => {
    const slow = deferred<SearchHit[]>();
    const engine = fakeEngine({
      search: (_document, query) => (query === 'alpha' ? slow.promise : Promise.resolve([hit(9)])),
    });
    const { result } = renderHook(() => useSearch(engine, 1, () => {}));
    act(() => result.current.submit('alpha', false));
    act(() => result.current.submit('beta', false));
    await waitFor(() => expect(result.current.state.kind).toBe('found'));
    await act(async () => slow.resolve([hit(1)]));
    expect(result.current.state).toMatchObject({ query: 'beta' });
    expect(currentHit(result.current.state)?.pageIndex).toBe(9);
  });

  it('clears on an empty query without asking the engine', () => {
    const search = vi.fn();
    const { result } = renderHook(() => useSearch(fakeEngine({ search }), 1, () => {}));
    act(() => result.current.submit('', false));
    expect(result.current.state).toEqual({ kind: 'idle' });
    expect(search).not.toHaveBeenCalled();
  });

  it('reports TextExtractionDenied to the viewer instead of as a failure', async () => {
    const onDenied = vi.fn();
    const engine = fakeEngine({
      search: () => Promise.reject(new EngineError('TextExtractionDenied', 'text extraction is not permitted')),
    });
    const { result } = renderHook(() => useSearch(engine, 1, onDenied));
    act(() => result.current.submit('beta', false));
    await waitFor(() => expect(onDenied).toHaveBeenCalledOnce());
    expect(result.current.state).toEqual({ kind: 'idle' });
  });

  it('shows any other failure', async () => {
    const engine = fakeEngine({ search: () => Promise.reject(new EngineError('RenderFailed', 'pdfium said no')) });
    const { result } = renderHook(() => useSearch(engine, 1, () => {}));
    act(() => result.current.submit('beta', false));
    await waitFor(() => expect(result.current.state).toEqual({ kind: 'failed', query: 'beta', message: 'pdfium said no' }));
  });
});
