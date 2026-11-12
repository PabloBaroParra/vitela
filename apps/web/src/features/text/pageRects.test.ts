import { describe, expect, it } from 'vitest';
import { boundingBox, toCssRects, toPagePoint } from './pageRects';

describe('toCssRects', () => {
  it('scales every rect and ignores a trailing partial one', () => {
    const rects = new Float32Array([10, 20, 30, 40, 1, 2, 3, 4, 99]);
    expect(toCssRects(rects, 2)).toEqual([
      { left: 20, top: 40, width: 60, height: 80 },
      { left: 2, top: 4, width: 6, height: 8 },
    ]);
  });

  it('is empty for no rects', () => {
    expect(toCssRects(new Float32Array(0), 2)).toEqual([]);
  });
});

describe('toPagePoint', () => {
  it('undoes the scale', () => {
    expect(toPagePoint(30, 60, 1.5)).toEqual({ x: 20, y: 40 });
  });

  it('round-trips a rect corner at any zoom', () => {
    const scale = (96 / 72) * 1.75;
    const [corner] = toCssRects(new Float32Array([72, 700, 10, 10]), scale);
    expect(toPagePoint(corner!.left, corner!.top, scale).x).toBeCloseTo(72);
    expect(toPagePoint(corner!.left, corner!.top, scale).y).toBeCloseTo(700);
  });
});

describe('boundingBox', () => {
  it('holds every rect', () => {
    expect(
      boundingBox([
        { left: 10, top: 50, width: 100, height: 10 },
        { left: 0, top: 62, width: 40, height: 10 },
      ]),
    ).toEqual({ left: 0, top: 50, width: 110, height: 22 });
  });

  it('is null with no rects', () => {
    expect(boundingBox([])).toBeNull();
  });
});
