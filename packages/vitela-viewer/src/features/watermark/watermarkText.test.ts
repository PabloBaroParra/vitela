import { describe, expect, it } from 'vitest';
import type { WatermarkDefaults } from '@vitela/core';
import {
  PICTURE_MISSING,
  TEXT_MISSING,
  draftOf,
  markRefusal,
  pictureSummary,
  switchKind,
  watermarkEdit,
  watermarkStatus,
} from './watermarkText';

const look: WatermarkDefaults = {
  fontSize: 60,
  textOpacity: 0.3,
  textRotationDegrees: 45,
  textBehind: false,
  pictureScale: 0.5,
  pictureOpacity: 0.3,
  pictureRotationDegrees: 0,
  pictureBehind: true,
};

describe('watermark draft', () => {
  it("starts as the core's text look on every page, with the picture's size ready", () => {
    expect(draftOf(look)).toEqual({
      kind: 'text',
      text: 'DRAFT',
      picture: null,
      pages: 'all',
      customRange: '',
      fontSize: 60,
      scalePercent: 50,
      opacityPercent: 30,
      rotationDegrees: 45,
      behind: false,
    });
  });

  it('keeps a core look outside the sliders inside them', () => {
    expect(draftOf({ ...look, fontSize: 500, textOpacity: 0.01 })).toMatchObject({ fontSize: 200, opacityPercent: 5 });
  });

  it("switching kind resets only the angle and the placement to that kind's defaults", () => {
    const text = { ...draftOf(look), opacityPercent: 70, rotationDegrees: 10, pages: 'current' as const };
    const picture = switchKind(text, 'picture', look);
    expect(picture).toEqual({ ...text, kind: 'picture', rotationDegrees: 0, behind: true });
    expect(switchKind(picture, 'text', look)).toEqual({ ...picture, kind: 'text', rotationDegrees: 45, behind: false });
    expect(switchKind(text, 'text', look)).toBe(text);
  });

  it('refuses blank text and a missing picture, and nothing else', () => {
    expect(markRefusal({ ...draftOf(look), text: '   ' })).toBe(TEXT_MISSING);
    expect(markRefusal({ ...draftOf(look), text: ' DRAFT' })).toBeNull();
    const picture = switchKind(draftOf(look), 'picture', look);
    expect(markRefusal(picture)).toBe(PICTURE_MISSING);
    expect(markRefusal({ ...picture, picture: { name: 'logo.png', bytes: new Uint8Array(1) } })).toBeNull();
  });

  it('becomes the edit: fractions for opacity and scale, the text as typed', () => {
    expect(watermarkEdit({ ...draftOf(look), text: ' Copy ', opacityPercent: 40 }, [0, 2])).toEqual({
      kind: 'addWatermark',
      pages: [0, 2],
      mark: { kind: 'text', text: ' Copy ', fontSize: 60 },
      opacity: 0.4,
      rotationDegrees: 45,
      behind: false,
    });
    const bytes = new Uint8Array([1, 2]);
    const picture = { ...switchKind(draftOf(look), 'picture', look), picture: { name: 'logo.png', bytes }, scalePercent: 75 };
    expect(watermarkEdit(picture, [1])).toEqual({
      kind: 'addWatermark',
      pages: [1],
      mark: { kind: 'picture', image: bytes, scale: 0.75 },
      opacity: 0.3,
      rotationDegrees: 0,
      behind: true,
    });
  });

  it('says what was picked and how many pages were stamped', () => {
    expect(pictureSummary(null)).toBe('No picture chosen. PNG or JPEG.');
    expect(pictureSummary({ name: 'logo.png', bytes: new Uint8Array(3000) })).toBe('logo.png (3 KB).');
    expect(pictureSummary({ name: 'dot.png', bytes: new Uint8Array(10) })).toBe('dot.png (1 KB).');
    expect(watermarkStatus(1)).toBe('Watermark added to 1 page.');
    expect(watermarkStatus(4)).toBe('Watermark added to 4 pages.');
  });
});
