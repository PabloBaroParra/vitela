import { describe, expect, it } from 'vitest';
import { THUMBNAIL_BOX, thumbnailSize } from './thumbnailSize';

describe('thumbnailSize', () => {
  it('fits a portrait page to the box height', () => {
    const size = thumbnailSize({ widthPt: 612, heightPt: 792, rotationDegrees: 0 }, 1);
    expect(size.height).toBeCloseTo(THUMBNAIL_BOX.height);
    expect(size.width).toBeLessThanOrEqual(THUMBNAIL_BOX.width);
    // One device pixel per CSS pixel, as a whole DPI the core renders at.
    expect(size.dpi).toBe(Math.floor((72 * THUMBNAIL_BOX.height) / 792));
  });

  it('fits a turned page to the box width and doubles the DPI on a 2x screen', () => {
    const landscape = { widthPt: 792, heightPt: 612, rotationDegrees: 90 };
    const size = thumbnailSize(landscape, 2);
    expect(size.width).toBeCloseTo(THUMBNAIL_BOX.width);
    expect(size.dpi).toBe(Math.floor((2 * 72 * THUMBNAIL_BOX.width) / 792));
  });

  it('never asks for less than 1 DPI', () => {
    expect(thumbnailSize({ widthPt: 14400, heightPt: 14400, rotationDegrees: 0 }, 1).dpi).toBe(1);
  });
});
