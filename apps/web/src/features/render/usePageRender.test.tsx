import { renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { Engine, RenderedPage } from '../../core/types';
import { usePageRender } from './usePageRender';

const page: RenderedPage = { width: 1, height: 1, rgba: new Uint8ClampedArray(4) };

function engineWith(renderPage: Engine['renderPage']): Engine {
  return { open: () => Promise.reject(new Error('unused')), renderPage };
}

describe('usePageRender', () => {
  it('renders nothing while there is no document', () => {
    const calls: unknown[] = [];
    const engine = engineWith(async (...args) => {
      calls.push(args);
      return page;
    });
    const { result } = renderHook(() => usePageRender(engine, null, 0, 144));
    expect(result.current).toEqual({ status: 'idle' });
    expect(calls).toHaveLength(0);
  });

  it('renders the requested page of the open document at the requested dpi', async () => {
    const calls: [number, number][] = [];
    const engine = engineWith(async (index, dpi) => {
      calls.push([index, dpi]);
      return page;
    });
    const { result } = renderHook(() => usePageRender(engine, 1, 0, 144));
    expect(result.current).toEqual({ status: 'rendering' });
    await waitFor(() => expect(result.current).toEqual({ status: 'rendered', page }));
    expect(calls).toEqual([[0, 144]]);
  });

  it('re-renders when a new document generation opens', async () => {
    let calls = 0;
    const engine = engineWith(async () => {
      calls++;
      return page;
    });
    const { result, rerender } = renderHook(({ generation }) => usePageRender(engine, generation, 0, 144), {
      initialProps: { generation: 1 },
    });
    await waitFor(() => expect(result.current.status).toBe('rendered'));
    rerender({ generation: 2 });
    await waitFor(() => expect(calls).toBe(2));
  });

  it('reports a failed render with the engine message', async () => {
    const engine = engineWith(() => Promise.reject(new Error('render: out of memory')));
    const { result } = renderHook(() => usePageRender(engine, 1, 0, 144));
    await waitFor(() => expect(result.current).toEqual({ status: 'failed', message: 'render: out of memory' }));
  });

  it('keeps showing the previous bitmap while a new dpi renders, then swaps it', async () => {
    const sharp: RenderedPage = { width: 2, height: 2, rgba: new Uint8ClampedArray(16) };
    let resolveSharp!: (page: RenderedPage) => void;
    const engine = engineWith((_index, dpi) =>
      dpi === 72 ? Promise.resolve(page) : new Promise((resolve) => (resolveSharp = resolve)),
    );
    const { result, rerender } = renderHook(({ dpi }) => usePageRender(engine, 1, 0, dpi), {
      initialProps: { dpi: 72 },
    });
    await waitFor(() => expect(result.current).toEqual({ status: 'rendered', page }));

    rerender({ dpi: 144 });
    expect(result.current).toEqual({ status: 'rendering', previous: page });
    resolveSharp(sharp);
    await waitFor(() => expect(result.current).toEqual({ status: 'rendered', page: sharp }));
  });

  it('ignores a render that finishes after the dpi moved on', async () => {
    const pending: ((page: RenderedPage) => void)[] = [];
    const engine = engineWith(() => new Promise((resolve) => pending.push(resolve)));
    const { result, rerender } = renderHook(({ dpi }) => usePageRender(engine, 1, 0, dpi), {
      initialProps: { dpi: 72 },
    });
    rerender({ dpi: 144 });
    const late: RenderedPage = { width: 1, height: 2, rgba: new Uint8ClampedArray(8) };
    const current: RenderedPage = { width: 2, height: 4, rgba: new Uint8ClampedArray(32) };
    pending[1]!(current);
    await waitFor(() => expect(result.current).toEqual({ status: 'rendered', page: current }));
    pending[0]!(late);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(result.current).toEqual({ status: 'rendered', page: current });
  });

  it('never carries a bitmap over to another document', async () => {
    const engine = engineWith(() => Promise.resolve(page));
    const { result, rerender } = renderHook(({ generation }) => usePageRender(engine, generation, 0, 72), {
      initialProps: { generation: 1 },
    });
    await waitFor(() => expect(result.current.status).toBe('rendered'));
    rerender({ generation: 2 });
    expect(result.current).toEqual({ status: 'rendering' });
  });
});
