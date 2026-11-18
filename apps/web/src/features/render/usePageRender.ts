// Asks the engine for one page whenever the document (or the page/dpi)
// changes. Container logic only; PageCanvas does the painting.

import { useEffect, useRef, useState } from 'react';
import type { DocumentHandle, Engine, RenderedPage } from '@vitela/core';

export type PageRenderState =
  | { readonly status: 'idle' }
  /**
   * A render is in flight. `previous` is the last bitmap of this page, kept
   * on screen (scaled) until the new one arrives, so zooming never blanks.
   */
  | { readonly status: 'rendering'; readonly previous?: RenderedPage }
  | { readonly status: 'rendered'; readonly page: RenderedPage }
  | { readonly status: 'failed'; readonly message: string };

/**
 * `document` is the engine handle of the document to render; `null` means
 * none is open. Handles are never reused, so it also tells documents apart.
 * `pageKey` names what is at `pageIndex` (the page's id and turn): when an
 * edit, undo or redo puts another page there, it changes and the page is
 * painted again, without the old bitmap in the meantime.
 */
export function usePageRender(
  engine: Engine,
  document: DocumentHandle | null,
  pageIndex: number,
  dpi: number,
  pageKey = '',
): PageRenderState {
  const [state, setState] = useState<PageRenderState>({ status: 'idle' });
  /** The document and page the bitmap in `state` belongs to. */
  const shown = useRef<string | null>(null);

  useEffect(() => {
    if (document === null) {
      setState({ status: 'idle' });
      return;
    }
    // A render that finishes after the inputs moved on is dropped.
    let stale = false;
    // Another document's (or page's) bitmap is never shown in place of this one.
    const identity = `${document}|${pageKey}`;
    const samePage = shown.current === identity;
    shown.current = identity;
    setState((prior) => {
      if (!samePage) return { status: 'rendering' };
      const previous = prior.status === 'rendered' ? prior.page : prior.status === 'rendering' ? prior.previous : undefined;
      return previous ? { status: 'rendering', previous } : { status: 'rendering' };
    });
    engine.renderPage(document, pageIndex, dpi).then(
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
  }, [engine, document, pageIndex, dpi, pageKey]);

  return document === null ? { status: 'idle' } : state;
}
