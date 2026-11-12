// The resolution a page is rendered at for a given zoom. The core renders
// at a whole DPI (the contract every shell shares), so this rounds.

import type { PageSize } from '../../core/types';
import { CSS_PX_PER_PT } from './layout';

/**
 * Pixel budget for one page bitmap. Below Safari's 16,777,216-pixel canvas
 * limit and far below the core's own 32 Mpx ceiling; at 4 bytes per pixel a
 * page never costs more than 48 MB to transfer.
 */
export const MAX_RENDER_PIXELS = 12_000_000;

/** One device pixel per CSS pixel, capped by `MAX_RENDER_PIXELS`. */
export function renderDpi(page: PageSize, zoom: number, devicePixelRatio: number): number {
  const wanted = Math.round(72 * CSS_PX_PER_PT * zoom * devicePixelRatio);
  let dpi = Math.max(1, Math.min(wanted, Math.floor(72 * Math.sqrt(MAX_RENDER_PIXELS / (page.widthPt * page.heightPt)))));
  // The core sizes a bitmap as ceil(points * dpi / 72) per side, which can
  // tip the estimate above over the budget.
  while (dpi > 1 && bitmapPixels(page, dpi) > MAX_RENDER_PIXELS) dpi--;
  return dpi;
}

function bitmapPixels(page: PageSize, dpi: number): number {
  return Math.ceil((page.widthPt * dpi) / 72) * Math.ceil((page.heightPt * dpi) / 72);
}
