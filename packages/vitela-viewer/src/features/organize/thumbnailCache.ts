// Rendered thumbnails, kept by page identity so that a reorder or a deletion
// renders nothing again (the Linux shell's lesson, organize/cache.rs).
//
// The key is the core's `PageId`, the page's turn and the DPI — not its
// position: positions change with every move, the id never does. A rotation
// changes the pixels, and it changes the key with them, so nothing has to be
// invalidated by hand; an undo of that rotation finds the old bitmap again.
// The cache belongs to one open document (the Organize screen makes one per
// mount), so ids never collide across documents.
//
// A render is requested through the cache and lands in it even if the card
// that asked was unmounted or moved meanwhile: the result is still right for
// its key. What the cache cannot know is whether the page at an index is
// still the page of a key — so callers must not request while an edit is in
// flight (see `useThumbnail`'s `paused`).

import type { PageSize, RenderedPage } from '@vitela/core';

export function thumbnailKey(pageId: number, page: PageSize, dpi: number): string {
  return `${pageId}/${page.rotationDegrees}/${dpi}`;
}

/** Enough for a few hundred thumbnails at 1x, or about a hundred at 2x. */
export const THUMBNAIL_BUDGET_BYTES = 64 * 1024 * 1024;

export class ThumbnailCache {
  /** Most recently used last. */
  private readonly done = new Map<string, RenderedPage>();
  private readonly inFlight = new Map<string, Promise<RenderedPage>>();
  private bytes = 0;

  constructor(private readonly budgetBytes: number = THUMBNAIL_BUDGET_BYTES) {}

  /** The bitmap of `key` if it is rendered, marking it recently used. */
  peek(key: string): RenderedPage | undefined {
    const page = this.done.get(key);
    if (page) {
      this.done.delete(key);
      this.done.set(key, page);
    }
    return page;
  }

  /** The bitmap of `key`: cached, already being rendered, or rendered now by `render`. */
  request(key: string, render: () => Promise<RenderedPage>): Promise<RenderedPage> {
    const cached = this.peek(key);
    if (cached) return Promise.resolve(cached);
    const pending = this.inFlight.get(key);
    if (pending) return pending;
    const started = render().then(
      (page) => {
        this.inFlight.delete(key);
        this.store(key, page);
        return page;
      },
      (error: unknown) => {
        this.inFlight.delete(key);
        throw error;
      },
    );
    this.inFlight.set(key, started);
    return started;
  }

  private store(key: string, page: RenderedPage): void {
    this.done.set(key, page);
    this.bytes += page.rgba.byteLength;
    for (const [oldest, evicted] of this.done) {
      if (this.bytes <= this.budgetBytes || oldest === key) break;
      this.done.delete(oldest);
      this.bytes -= evicted.rgba.byteLength;
    }
  }
}
