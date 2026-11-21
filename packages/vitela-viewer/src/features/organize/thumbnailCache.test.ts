import { describe, expect, it } from 'vitest';
import type { RenderedPage } from '@vitela/core';
import { ThumbnailCache, thumbnailKey } from './thumbnailCache';

const bitmap = (bytes: number): RenderedPage => ({ width: bytes / 4, height: 1, rgba: new Uint8ClampedArray(bytes) });

describe('thumbnailKey', () => {
  it('names the page by identity, its turn and the DPI', () => {
    const page = { widthPt: 612, heightPt: 792, rotationDegrees: 90 };
    expect(thumbnailKey(7, page, 20)).toBe('7/90/20');
    expect(thumbnailKey(7, { ...page, rotationDegrees: 0 }, 20)).not.toBe(thumbnailKey(7, page, 20));
  });
});

describe('ThumbnailCache', () => {
  it('renders a key once and serves it afterwards', async () => {
    const cache = new ThumbnailCache(1000);
    let renders = 0;
    const render = async () => (renders++, bitmap(40));
    const first = cache.request('a', render);
    // A second request while the first is in flight shares it.
    const second = cache.request('a', render);
    expect(await first).toBe(await second);
    expect(cache.peek('a')).toBe(await first);
    await cache.request('a', render);
    expect(renders).toBe(1);
  });

  it('keeps a render that lands after nobody waits for it', async () => {
    const cache = new ThumbnailCache(1000);
    let land!: (page: RenderedPage) => void;
    void cache.request('a', () => new Promise((resolve) => (land = resolve)));
    const page = bitmap(40);
    land(page);
    await Promise.resolve();
    await Promise.resolve();
    expect(cache.peek('a')).toBe(page);
  });

  it('forgets a failed render, so the next request tries again', async () => {
    const cache = new ThumbnailCache(1000);
    await expect(cache.request('a', () => Promise.reject(new Error('boom')))).rejects.toThrow('boom');
    expect(cache.peek('a')).toBeUndefined();
    await expect(cache.request('a', async () => bitmap(4))).resolves.toBeDefined();
  });

  it('evicts the least recently used bitmaps past its byte budget', async () => {
    const cache = new ThumbnailCache(100);
    await cache.request('a', async () => bitmap(40));
    await cache.request('b', async () => bitmap(40));
    cache.peek('a'); // a is now the most recent
    await cache.request('c', async () => bitmap(40));
    expect(cache.peek('b')).toBeUndefined();
    expect(cache.peek('a')).toBeDefined();
    expect(cache.peek('c')).toBeDefined();
  });
});
