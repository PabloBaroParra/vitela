// Container: renders the first page of the open document and shows it.

import type { Engine } from '../../core/types';
import { StatusLine } from '../../ui/StatusLine';
import { PageCanvas } from './PageCanvas';
import { usePageRender } from './usePageRender';

/** Fixed resolution until zoom lands (B26 T-221): twice one pixel per point. */
const RENDER_DPI = 144;

interface PageViewProps {
  readonly engine: Engine;
  readonly generation: number;
  readonly name: string;
}

export function PageView({ engine, generation, name }: PageViewProps) {
  const render = usePageRender(engine, generation, 0, RENDER_DPI);
  switch (render.status) {
    case 'idle':
    case 'rendering':
      return <StatusLine>Rendering…</StatusLine>;
    case 'failed':
      return <StatusLine tone="error">{`Could not render ${name}: ${render.message}`}</StatusLine>;
    case 'rendered':
      return <PageCanvas page={render.page} label={`Page 1 of ${name}`} />;
  }
}
