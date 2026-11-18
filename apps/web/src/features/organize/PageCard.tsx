// One page in the Organize grid's Pages view: its thumbnail and its position.
// The card is an option of the grid's listbox; the grid owns focus and
// selection.

import { memo } from 'react';
import { ThumbnailBox, ThumbnailError, useCardThumbnail, type CardPage } from './CardThumbnail';
import type { GridItem } from './ThumbnailGrid';

interface PageCardProps extends GridItem {
  readonly thumbnail: CardPage;
}

/** The DOM id of a page card, for the grid's `aria-activedescendant`. */
export function cardId(pageId: number): string {
  return `organize-page-${pageId}`;
}

export const PageCard = memo(function PageCard(props: PageCardProps) {
  const { index, rect, selected, active } = props;
  const { pageId, page } = props.thumbnail;
  const { size, thumbnail } = useCardThumbnail(props.thumbnail);
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
      <ThumbnailBox size={size} thumbnail={thumbnail} label={`Thumbnail of page ${number}`} />
      <span className="page-card__number" aria-hidden="true">
        {number}
      </span>
      <ThumbnailError thumbnail={thumbnail} />
    </div>
  );
});
