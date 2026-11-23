import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { useEngineStatus } from './useEngineStatus';

describe('useEngineStatus', () => {
  it('is loading, then ready once the engine says so', async () => {
    const loaded = { ready: Promise.resolve() };
    const { result } = renderHook(() => useEngineStatus(loaded));
    expect(result.current).toEqual({ kind: 'loading' });
    await act(async () => {});
    expect(result.current).toEqual({ kind: 'ready' });
  });

  it('reports a failed load with its message', async () => {
    const failing = { ready: Promise.reject(new Error('wasm blocked')) };
    const { result } = renderHook(() => useEngineStatus(failing));
    await act(async () => {});
    expect(result.current).toEqual({ kind: 'failed', message: 'wasm blocked' });
  });

  it('is loading again for a new engine until that one says it is ready', async () => {
    let loaded!: () => void;
    const first = { ready: Promise.resolve() };
    const second = { ready: new Promise<void>((resolve) => (loaded = resolve)) };
    const { result, rerender } = renderHook(({ client }) => useEngineStatus(client), {
      initialProps: { client: first },
    });
    await act(async () => {});
    expect(result.current).toEqual({ kind: 'ready' });

    rerender({ client: second });
    expect(result.current).toEqual({ kind: 'loading' });
    await act(async () => loaded());
    expect(result.current).toEqual({ kind: 'ready' });
  });
});
