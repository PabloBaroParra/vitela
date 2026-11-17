// One card's thumbnail: served from the cache when it is there, rendered
// through it otherwise. Container logic only; the card paints the pixels.
//
// `paused` holds back new renders while an edit is in flight: until its
// answer arrives the UI still has the old page order, and a render asked by
// position now would reach the edited preview and be filed under the wrong
// page. Renders already sent are fine — the worker answers in order.

import { useEffect, useRef, useState } from 'react';
import { errorMessage, type RenderedPage } from '@vitela/core';
import type { ThumbnailCache } from './thumbnailCache';

export type ThumbnailState =
  | { readonly status: 'rendering' }
  | { readonly status: 'rendered'; readonly page: RenderedPage }
  | { readonly status: 'failed'; readonly message: string };

export function useThumbnail(
  cache: ThumbnailCache,
  key: string,
  render: () => Promise<RenderedPage>,
  paused: boolean,
): ThumbnailState {
  const [state, setState] = useState<{ key: string; value: ThumbnailState } | null>(null);
  // The latest render function, without making every new closure a new render.
  const latest = useRef(render);
  useEffect(() => {
    latest.current = render;
  });

  const cached = cache.peek(key);
  useEffect(() => {
    if (cached || paused) return;
    let current = true;
    cache.request(key, () => latest.current()).then(
      (page) => current && setState({ key, value: { status: 'rendered', page } }),
      (error: unknown) => current && setState({ key, value: { status: 'failed', message: errorMessage(error) } }),
    );
    return () => {
      current = false;
    };
  }, [cache, key, paused, cached]);

  if (cached) return { status: 'rendered', page: cached };
  return state?.key === key ? state.value : { status: 'rendering' };
}
