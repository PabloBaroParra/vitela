// Puts a rendered page's pixels into a canvas, at 1:1 device pixels.

import type { RenderedPage } from '../../core/types';

export function paintPage(canvas: HTMLCanvasElement, page: RenderedPage): void {
  const expected = page.width * page.height * 4;
  if (page.rgba.length !== expected) {
    throw new Error(`page is ${page.width}x${page.height}: expected ${expected} bytes, got ${page.rgba.length}`);
  }
  const context = canvas.getContext('2d');
  if (!context) throw new Error('canvas has no 2D context');
  canvas.width = page.width;
  canvas.height = page.height;
  context.putImageData(new ImageData(page.rgba as Uint8ClampedArray<ArrayBuffer>, page.width, page.height), 0, 0);
}
