import { beforeEach, describe, expect, it, vi } from 'vitest';
import { paintPage } from './paint';

// jsdom has no ImageData; every browser the app targets does.
class FakeImageData {
  constructor(
    readonly data: Uint8ClampedArray,
    readonly width: number,
    readonly height: number,
  ) {}
}
beforeEach(() => vi.stubGlobal('ImageData', FakeImageData));

describe('paintPage', () => {
  it('sizes the canvas to the page and puts its pixels at the origin', () => {
    const puts: [ImageData, number, number][] = [];
    const context = { putImageData: (data: ImageData, x: number, y: number) => puts.push([data, x, y]) };
    const canvas = { width: 0, height: 0, getContext: () => context } as unknown as HTMLCanvasElement;
    const rgba = new Uint8ClampedArray([1, 2, 3, 4, 5, 6, 7, 8]);

    paintPage(canvas, { width: 2, height: 1, rgba });

    expect([canvas.width, canvas.height]).toEqual([2, 1]);
    expect(puts).toHaveLength(1);
    const [[image, x, y]] = puts as [[ImageData, number, number]];
    expect([image.width, image.height, x, y]).toEqual([2, 1, 0, 0]);
    expect([...image.data]).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
  });

  it('refuses pixels that do not match the page size', () => {
    const canvas = { getContext: () => ({ putImageData: () => {} }) } as unknown as HTMLCanvasElement;
    expect(() => paintPage(canvas, { width: 2, height: 2, rgba: new Uint8ClampedArray(8) })).toThrow(
      /16 bytes/,
    );
  });

  it('fails loudly when the canvas has no 2D context', () => {
    const canvas = { getContext: () => null } as unknown as HTMLCanvasElement;
    expect(() => paintPage(canvas, { width: 1, height: 1, rgba: new Uint8ClampedArray(4) })).toThrow(
      /2D context/,
    );
  });
});
