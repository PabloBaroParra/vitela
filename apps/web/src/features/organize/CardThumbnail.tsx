// The thumbnail every Organize card shows: a page card its own page, a block
// card its first page. Kept by `PageId` in the screen's cache (see
// `thumbnailCache.ts`), so both views share the bitmaps.

import type { DocumentHandle, Engine, PageSize } from '@vitela/core';
import { PageCanvas } from '../render/PageCanvas';
import { thumbnailKey, type ThumbnailCache } from './thumbnailCache';
import { thumbnailSize } from './thumbnailSize';
import { useThumbnail, type ThumbnailState } from './useThumbnail';

export interface CardPage {
  readonly engine: Engine;
  readonly document: DocumentHandle;
  readonly cache: ThumbnailCache;
  /** Where the page is now: what the render asks for. */
  readonly pageIndex: number;
  readonly pageId: number;
  readonly page: PageSize;
  readonly devicePixelRatio: number;
  /** No new renders while an edit is in flight (see `useThumbnail`). */
  readonly paused: boolean;
}

/** Container logic: the page's thumbnail state and the box it is drawn in. */
export function useCardThumbnail({ engine, document, cache, pageIndex, pageId, page, devicePixelRatio, paused }: CardPage) {
  const size = thumbnailSize(page, devicePixelRatio);
  const thumbnail = useThumbnail(
    cache,
    thumbnailKey(pageId, page, size.dpi),
    () => engine.renderPage(document, pageIndex, size.dpi),
    paused,
  );
  return { size, thumbnail };
}

interface ThumbnailBoxProps {
  readonly size: { readonly width: number; readonly height: number };
  readonly thumbnail: ThumbnailState;
  readonly label: string;
}

/** Presentational: the page at its aspect ratio, its pixels once they are in. */
export function ThumbnailBox({ size, thumbnail, label }: ThumbnailBoxProps) {
  return (
    <div className="page-card__box">
      <div className="page-card__page" style={{ width: size.width, height: size.height }}>
        {thumbnail.status === 'rendered' && <PageCanvas page={thumbnail.page} label={label} />}
      </div>
    </div>
  );
}

export function ThumbnailError({ thumbnail }: { readonly thumbnail: ThumbnailState }) {
  return thumbnail.status === 'failed' ? (
    <span className="page-card__error">{`Could not render: ${thumbnail.message}`}</span>
  ) : null;
}
