import { describe, expect, it } from 'vitest';
import type { PageSize } from '../../core/types';
import { MAX_RENDER_PIXELS, renderDpi } from './renderDpi';

const letter: PageSize = { widthPt: 612, heightPt: 792, rotationDegrees: 0 };

describe('renderDpi', () => {
  it('renders one device pixel per CSS pixel at the current zoom', () => {
    expect(renderDpi(letter, 1, 1)).toBe(96);
    expect(renderDpi(letter, 1.5, 1)).toBe(144);
    expect(renderDpi(letter, 1, 2)).toBe(192);
  });

  it('is always a whole number, as the core requires', () => {
    expect(Number.isInteger(renderDpi(letter, 1.1, 1.25))).toBe(true);
  });

  it('caps the bitmap of a large page under the pixel budget', () => {
    const dpi = renderDpi(letter, 4, 3);
    const pixels = Math.ceil((612 * dpi) / 72) * Math.ceil((792 * dpi) / 72);
    expect(dpi).toBeLessThan(96 * 4 * 3);
    expect(pixels).toBeLessThanOrEqual(MAX_RENDER_PIXELS);
  });

  it('never asks for less than one dot per inch', () => {
    expect(renderDpi({ widthPt: 1, heightPt: 1_000_000, rotationDegrees: 0 }, 1, 1)).toBeGreaterThanOrEqual(1);
  });
});
