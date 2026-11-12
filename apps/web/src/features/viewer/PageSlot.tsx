// One page's place in the viewer: sized and positioned by the layout,
// rendered at `dpi` while it is mounted, with its text layer (search
// highlights, selection) on top. The viewer mounts only the slots near the
// viewport, so far-away pages hold no canvas and no text layer at all.

import { memo } from 'react';
import type { DocumentHandle, Engine, PageRects } from '../../core/types';
import { PageCanvas } from '../render/PageCanvas';
import { usePageRender } from '../render/usePageRender';
import { TextLayer } from '../text/TextLayer';
import type { TextInput } from '../text/useTextSelection';
import type { Rect } from './layout';

interface PageSlotProps {
  readonly engine: Engine;
  readonly document: DocumentHandle;
  readonly index: number;
  readonly slot: Rect;
  readonly dpi: number;
  readonly label: string;
  /** CSS px per PDF point at the current zoom. */
  readonly scale: number;
  readonly hits?: readonly PageRects[];
  readonly currentHit?: PageRects;
  readonly selection?: PageRects;
  readonly textInput: TextInput | null;
}

export const PageSlot = memo(function PageSlot(props: PageSlotProps) {
  const { engine, document, index, slot, dpi, label, scale, hits, currentHit, selection, textInput } = props;
  const render = usePageRender(engine, document, index, dpi);
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
      <TextLayer
        pageIndex={index}
        scale={scale}
        hits={hits}
        currentHit={currentHit}
        selection={selection}
        input={textInput}
      />
      {render.status === 'failed' && (
        <p className="page-slot__error" role="alert">{`Could not render this page: ${render.message}`}</p>
      )}
    </div>
  );
});
