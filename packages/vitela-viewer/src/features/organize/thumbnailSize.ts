// How big a page's thumbnail is drawn, and at what DPI the core renders it.
// Every shell renders thumbnails through the ordinary page render at a low
// DPI; the core's contract is a whole DPI. Pure.

import type { PageSize } from '@vitela/core';

/** The box a thumbnail fits in, in CSS px, inside its card. */
export const THUMBNAIL_BOX = { width: 144, height: 186 } as const;

export interface ThumbnailSize {
  /** The page fitted in the box, in CSS px. */
  readonly width: number;
  readonly height: number;
  /** One device pixel per CSS pixel. */
  readonly dpi: number;
}

/** `page` is as drawn (sizes already turned), so a rotated page fits as it looks. */
export function thumbnailSize(page: PageSize, devicePixelRatio: number): ThumbnailSize {
  const scale = Math.min(THUMBNAIL_BOX.width / page.widthPt, THUMBNAIL_BOX.height / page.heightPt);
  return {
    width: page.widthPt * scale,
    height: page.heightPt * scale,
    dpi: Math.max(1, Math.floor(72 * scale * devicePixelRatio)),
  };
}
