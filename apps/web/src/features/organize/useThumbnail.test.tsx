import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { RenderedPage } from '@vitela/core';
import { ThumbnailCache } from './thumbnailCache';
import { useThumbnail } from './useThumbnail';

const bitmap = (marker: number): RenderedPage => ({ width: 1, height: 1, rgba: new Uint8ClampedArray([marker, 0, 0, 255]) });

describe('useThumbnail', () => {
  it('renders a page it has not seen and shows the bitmap', async () => {
    const cache = new ThumbnailCache();
    const loads: string[] = [];
    const { result } = renderHook(() => useThumbnail(cache, 'a', async () => (loads.push('a'), bitmap(1)), false));
    expect(result.current.status).toBe('rendering');
    await waitFor(() => expect(result.current.status).toBe('rendered'));
    expect(result.current.status === 'rendered' && result.current.page.rgba[0]).toBe(1);
    expect(loads).toEqual(['a']);
  });

  it('shows a cached page at once, without rendering', () => {
    const cache = new ThumbnailCache();
    const page = bitmap(2);
    void cache.request('a', async () => page);
    return waitFor(() => expect(cache.peek('a')).toBe(page)).then(() => {
      const { result } = renderHook(() =>
        useThumbnail(cache, 'a', () => Promise.reject(new Error('not expected')), false),
      );
      expect(result.current).toEqual({ status: 'rendered', page });
    });
  });

  it('starts nothing while paused, and renders once resumed', async () => {
    const cache = new ThumbnailCache();
    const loads: string[] = [];
    const { result, rerender } = renderHook(
      ({ paused }) => useThumbnail(cache, 'a', async () => (loads.push('a'), bitmap(3)), paused),
      { initialProps: { paused: true } },
    );
    expect(result.current.status).toBe('rendering');
    expect(loads).toEqual([]);
    rerender({ paused: false });
    await waitFor(() => expect(result.current.status).toBe('rendered'));
    expect(loads).toEqual(['a']);
  });

  it('drops a render for a key it no longer shows', async () => {
    const cache = new ThumbnailCache();
    let land!: (page: RenderedPage) => void;
    const { result, rerender } = renderHook(
      ({ key }) =>
        useThumbnail(cache, key, key === 'a' ? () => new Promise((resolve) => (land = resolve)) : async () => bitmap(5), false),
      { initialProps: { key: 'a' } },
    );
    rerender({ key: 'b' });
    await waitFor(() => expect(result.current.status === 'rendered' && result.current.page.rgba[0]).toBe(5));
    await act(async () => land(bitmap(4)));
    expect(result.current.status === 'rendered' && result.current.page.rgba[0]).toBe(5);
  });

  it('reports a failed render', async () => {
    const cache = new ThumbnailCache();
    const { result } = renderHook(() => useThumbnail(cache, 'a', () => Promise.reject(new Error('boom')), false));
    await waitFor(() => expect(result.current).toEqual({ status: 'failed', message: 'boom' }));
  });
});
