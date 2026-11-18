// One block in the Organize grid's Documents view — a contiguous run of pages
// from one PDF: its first page's thumbnail, the PDF it came from and how many
// pages it holds. Like a page card, an option of the grid's listbox.

import { memo } from 'react';
import { ThumbnailBox, ThumbnailError, useCardThumbnail, type CardPage } from './CardThumbnail';
import type { GridItem } from './ThumbnailGrid';

interface BlockCardProps extends GridItem {
  /** The block's first page. */
  readonly thumbnail: CardPage;
  /** What the block is called (its PDF, and part). */
  readonly label: string;
  readonly pageCount: number;
}

/** The DOM id of a block card: its first page's id names it while it stays first. */
export function blockCardId(firstPageId: number): string {
  return `organize-block-${firstPageId}`;
}

export const BlockCard = memo(function BlockCard(props: BlockCardProps) {
  const { index, rect, selected, active, label, pageCount } = props;
  const { pageId, pageIndex } = props.thumbnail;
  const { size, thumbnail } = useCardThumbnail(props.thumbnail);
  const pages = `${pageCount} ${pageCount === 1 ? 'page' : 'pages'}`;
  return (
    <div
      id={blockCardId(pageId)}
      className="page-card block-card"
      role="option"
      aria-selected={selected}
      aria-label={`${label}, ${pages}`}
      style={{ top: rect.top, left: rect.left, width: rect.width, height: rect.height }}
      // E2E reads these: which pages the block holds and whether its pixels are in.
      data-index={index}
      data-start={pageIndex}
      data-pages={pageCount}
      data-state={thumbnail.status}
      data-active={active}
      onPointerDown={(event) => props.onPointerDown(index, event)}
      onClick={(event) => props.onClick(index, event)}
    >
      <ThumbnailBox size={size} thumbnail={thumbnail} label={`First page of ${label}`} />
      <span className="block-card__label" title={label}>
        {label}
      </span>
      <span className="block-card__pages">{pages}</span>
      <ThumbnailError thumbnail={thumbnail} />
    </div>
  );
});
