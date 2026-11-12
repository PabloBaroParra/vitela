// Asks the engine for one page whenever the document (or the page/dpi)
// changes. Container logic only; PageCanvas does the painting.

import { useEffect, useState } from 'react';
import type { Engine, RenderedPage } from '../../core/types';

export type PageRenderState =
  | { readonly status: 'idle' }
  | { readonly status: 'rendering' }
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

  useEffect(() => {
    if (generation === null) {
      setState({ status: 'idle' });
      return;
    }
    let stale = false;
    setState({ status: 'rendering' });
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
