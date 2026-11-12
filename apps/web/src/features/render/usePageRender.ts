// Asks the engine for one page whenever the document (or the page/dpi)
// changes. Container logic only; PageCanvas does the painting.

import { useEffect, useRef, useState } from 'react';
import type { Engine, RenderedPage } from '../../core/types';

export type PageRenderState =
  | { readonly status: 'idle' }
  /**
   * A render is in flight. `previous` is the last bitmap of this page, kept
   * on screen (scaled) until the new one arrives, so zooming never blanks.
   */
  | { readonly status: 'rendering'; readonly previous?: RenderedPage }
  | { readonly status: 'rendered'; readonly page: RenderedPage }
  | { readonly status: 'failed'; readonly message: string };

/** `generation` identifies the open document; `null` means none is open. */
export function usePageRender(
  engine: Engine,
  generation: number | null,
  pageIndex: number,
  dpi: number,
): PageRenderState {
  const [state, setState] = useState<PageRenderState>({ status: 'idle' });
  /** The document the bitmap in `state` belongs to. */
  const shown = useRef<number | null>(null);

  useEffect(() => {
    if (generation === null) {
      setState({ status: 'idle' });
      return;
    }
    // A render that finishes after the inputs moved on is dropped.
    let stale = false;
    // Another document's bitmap is never shown in place of this one.
    const sameDocument = shown.current === generation;
    shown.current = generation;
    setState((prior) => {
      if (!sameDocument) return { status: 'rendering' };
      const previous = prior.status === 'rendered' ? prior.page : prior.status === 'rendering' ? prior.previous : undefined;
      return previous ? { status: 'rendering', previous } : { status: 'rendering' };
    });
    engine.renderPage(pageIndex, dpi).then(
      (page) => {
        if (!stale) setState({ status: 'rendered', page });
      },
      (error: unknown) => {
        if (!stale) setState({ status: 'failed', message: error instanceof Error ? error.message : String(error) });
      },
    );
    return () => {
      stale = true;
    };
  }, [engine, generation, pageIndex, dpi]);

  return generation === null ? { status: 'idle' } : state;
}
