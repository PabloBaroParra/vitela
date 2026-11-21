import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useSettled } from './useSettled';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('useSettled', () => {
  it('follows the value only once it stops changing', () => {
    const { result, rerender } = renderHook(({ value }) => useSettled(value, 100), { initialProps: { value: 1 } });
    rerender({ value: 2 });
    act(() => vi.advanceTimersByTime(60));
    rerender({ value: 3 });
    act(() => vi.advanceTimersByTime(60));
    expect(result.current).toBe(1);
    act(() => vi.advanceTimersByTime(40));
    expect(result.current).toBe(3);
  });
});
