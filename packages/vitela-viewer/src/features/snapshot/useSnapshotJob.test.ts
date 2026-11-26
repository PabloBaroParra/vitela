import { renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { useSnapshotJob } from './useSnapshotJob';

describe('useSnapshotJob', () => {
  it('runs one job at a time until it is given back', () => {
    const { result } = renderHook(() => useSnapshotJob(1));
    expect(result.current.claim()).toBe(true);
    expect(result.current.claim()).toBe(false);
    result.current.release();
    expect(result.current.claim()).toBe(true);
  });

  it('drops a job once another document is shown, frees the slot and lets the caller tidy up', () => {
    const dispose = vi.fn();
    const { result, rerender } = renderHook(({ generation }) => useSnapshotJob(generation, dispose), { initialProps: { generation: 1 } });
    dispose.mockClear();
    expect(result.current.claim()).toBe(true);
    const guard = result.current.guard(1);
    expect(guard.current()).toBe(true);
    expect(result.current.isCurrent(1)).toBe(true);

    rerender({ generation: 2 });
    expect(guard.current()).toBe(false);
    expect(result.current.isCurrent(1)).toBe(false);
    expect(dispose).toHaveBeenCalledTimes(1);
    expect(result.current.claim()).toBe(true);
  });

  it('drops a job once the editor goes away, and lets the caller tidy up', () => {
    const dispose = vi.fn();
    const { result, unmount } = renderHook(() => useSnapshotJob(1, dispose));
    const guard = result.current.guard(1);
    dispose.mockClear();
    unmount();
    expect(guard.current()).toBe(false);
    expect(dispose).toHaveBeenCalledTimes(1);
  });

  it('hears Stop, and a new job starts unstopped', () => {
    const { result } = renderHook(() => useSnapshotJob(1));
    const first = result.current.guard(1);
    expect(first.stopped()).toBe(false);
    result.current.stop();
    expect(first.stopped()).toBe(true);
    const second = result.current.guard(1);
    expect(second.stopped()).toBe(false);
  });

  it('keeps the same controls across renders', () => {
    const { result, rerender } = renderHook(() => useSnapshotJob(1));
    const before = result.current;
    rerender();
    expect(result.current).toBe(before);
  });
});
