// One page's place in the viewer: sized and positioned by the layout,
// rendered at `dpi` while it is mounted. The viewer mounts only the slots
// near the viewport, so far-away pages hold no canvas at all.

import { memo } from 'react';
import type { Engine } from '../../core/types';
import { PageCanvas } from '../render/PageCanvas';
import { usePageRender } from '../render/usePageRender';
import type { Rect } from './layout';

interface PageSlotProps {
  readonly engine: Engine;
  readonly generation: number;
  readonly index: number;
  readonly slot: Rect;
  readonly dpi: number;
  readonly label: string;
}

export const PageSlot = memo(function PageSlot({ engine, generation, index, slot, dpi, label }: PageSlotProps) {
  const render = usePageRender(engine, generation, index, dpi);
  const shown = render.status === 'rendered' ? render.page : render.status === 'rendering' ? render.previous : undefined;
  return (
    <div
      className="page-slot"
      style={{ top: slot.top, left: slot.left, width: slot.width, height: slot.height }}
      // E2E waits on these: which page, and whether its bitmap is current.
      data-page={index + 1}
      data-state={render.status}
      data-dpi={dpi}
    >
      {shown && <PageCanvas page={shown} label={label} />}
      {render.status === 'failed' && (
        <p className="page-slot__error" role="alert">{`Could not render this page: ${render.message}`}</p>
      )}
    </div>
  );
});
