// One page in the Organize grid: its thumbnail and its position. The card is
// an option of the grid's listbox; the grid owns focus and selection.

import { memo, type MouseEvent, type PointerEvent } from 'react';
import type { DocumentHandle, Engine, PageSize } from '@vitela/core';
import { PageCanvas } from '../render/PageCanvas';
import type { Rect } from '../viewer/layout';
import { thumbnailKey, type ThumbnailCache } from './thumbnailCache';
import { thumbnailSize } from './thumbnailSize';
import { useThumbnail } from './useThumbnail';

interface PageCardProps {
  readonly engine: Engine;
  readonly document: DocumentHandle;
  readonly cache: ThumbnailCache;
  readonly index: number;
  readonly pageId: number;
  readonly page: PageSize;
  readonly rect: Rect;
  readonly devicePixelRatio: number;
  readonly selected: boolean;
  readonly active: boolean;
  /** No new renders while an edit is in flight (see `useThumbnail`). */
  readonly paused: boolean;
  readonly onPointerDown: (index: number, event: PointerEvent) => void;
  readonly onClick: (index: number, event: MouseEvent) => void;
}

/** The DOM id of a card, for the grid's `aria-activedescendant`. */
export function cardId(pageId: number): string {
  return `organize-page-${pageId}`;
}

export const PageCard = memo(function PageCard(props: PageCardProps) {
  const { engine, document, cache, index, pageId, page, rect, devicePixelRatio, selected, active, paused } = props;
  const size = thumbnailSize(page, devicePixelRatio);
  const thumbnail = useThumbnail(
    cache,
    thumbnailKey(pageId, page, size.dpi),
    () => engine.renderPage(document, index, size.dpi),
    paused,
  );
  const number = index + 1;
  return (
    <div
      id={cardId(pageId)}
      className="page-card"
      role="option"
      aria-selected={selected}
      aria-label={`Page ${number}`}
      style={{ top: rect.top, left: rect.left, width: rect.width, height: rect.height }}
      // E2E reads these: which page, its turn, and whether its pixels are in.
      data-index={index}
      data-page-id={pageId}
      data-rotation={page.rotationDegrees}
      data-state={thumbnail.status}
      data-active={active}
      onPointerDown={(event) => props.onPointerDown(index, event)}
      onClick={(event) => props.onClick(index, event)}
    >
      <div className="page-card__box">
        <div className="page-card__page" style={{ width: size.width, height: size.height }}>
          {thumbnail.status === 'rendered' && <PageCanvas page={thumbnail.page} label={`Thumbnail of page ${number}`} />}
        </div>
      </div>
      <span className="page-card__number" aria-hidden="true">
        {number}
      </span>
      {thumbnail.status === 'failed' && <span className="page-card__error">{`Could not render: ${thumbnail.message}`}</span>}
    </div>
  );
});
