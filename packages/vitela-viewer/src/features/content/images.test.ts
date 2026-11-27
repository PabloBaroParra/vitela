import { describe, expect, it } from 'vitest';
import type { ContentImage, PageContent } from '@vitela/core';
import { followImage, handleBox, IMAGE_HANDLES, MIN_IMAGE_PT, resizedRect } from './images';

const box = { left: 100, top: 200, width: 200, height: 100 };

describe('resizedRect', () => {
  it('keeps the proportions from a corner, its opposite corner fixed', () => {
    // Dragged right by 100: the width grows to 300, the height follows to 150.
    expect(resizedRect(box, 'se', 100, 0, false)).toEqual({ left: 100, top: 200, width: 300, height: 150 });
    // The larger of the two pulls: down by 100 doubles the height and so the width.
    expect(resizedRect(box, 'se', 0, 100, false)).toEqual({ left: 100, top: 200, width: 400, height: 200 });
    // From the top-left corner the bottom-right stays.
    expect(resizedRect(box, 'nw', 100, 0, false)).toEqual({ left: 200, top: 250, width: 100, height: 50 });
  });

  it('frees the proportions from a corner with Shift', () => {
    expect(resizedRect(box, 'se', 100, 10, true)).toEqual({ left: 100, top: 200, width: 300, height: 110 });
    expect(resizedRect(box, 'ne', -50, -20, true)).toEqual({ left: 100, top: 180, width: 150, height: 120 });
  });

  it('changes one side from an edge handle', () => {
    expect(resizedRect(box, 'e', 40, 999, false)).toEqual({ left: 100, top: 200, width: 240, height: 100 });
    expect(resizedRect(box, 'w', 40, 0, false)).toEqual({ left: 140, top: 200, width: 160, height: 100 });
    expect(resizedRect(box, 'n', 0, -30, false)).toEqual({ left: 100, top: 170, width: 200, height: 130 });
    expect(resizedRect(box, 's', 0, 30, true)).toEqual({ left: 100, top: 200, width: 200, height: 130 });
  });

  it('never shrinks a side below the floor, nor turns the box inside out', () => {
    const shrunk = resizedRect(box, 'e', -500, 0, false);
    expect(shrunk.width).toBe(MIN_IMAGE_PT);
    expect(shrunk.left).toBe(100);
    const corner = resizedRect(box, 'se', -500, -500, false);
    expect(corner.width).toBeGreaterThanOrEqual(MIN_IMAGE_PT);
    expect(corner.height).toBeGreaterThanOrEqual(MIN_IMAGE_PT);
    expect(corner.width / corner.height).toBeCloseTo(2, 5);
    const top = resizedRect(box, 'n', 0, 500, false);
    expect(top.height).toBe(MIN_IMAGE_PT);
    expect(top.top + top.height).toBe(300);
  });
});

describe('handleBox', () => {
  it('puts the eight handles on the corners and the middles of the sides, in CSS px', () => {
    expect(IMAGE_HANDLES).toHaveLength(8);
    const at = (handle: (typeof IMAGE_HANDLES)[number]) => {
      const { left, top, width } = handleBox(box, handle, 2);
      return [left + width / 2, top + width / 2];
    };
    expect(at('nw')).toEqual([200, 400]);
    expect(at('n')).toEqual([400, 400]);
    expect(at('e')).toEqual([600, 500]);
    expect(at('se')).toEqual([600, 600]);
    expect(at('w')).toEqual([200, 500]);
  });
});

describe('followImage', () => {
  const image = (id: number, rect: ContentImage['rect']): ContentImage => ({ kind: 'image', id, pageIndex: 0, rect, inline: false });
  const content = (images: ContentImage[]): PageContent => ({ runs: [], images, paths: [] });

  it('finds the edited image by where it should be now, the closest box winning', () => {
    const moved = { left: 130, top: 180, width: 200, height: 100 };
    const near = image(3, { left: 130.0001, top: 179.9999, width: 200, height: 100 });
    const overlapping = image(4, { left: 140, top: 190, width: 200, height: 100 });
    expect(followImage(content([overlapping, near]), moved)).toBe(near);
  });

  it('finds nothing for a removed image, or where nothing overlaps', () => {
    expect(followImage(content([image(1, box)]), null)).toBeNull();
    expect(followImage(content([image(1, box)]), { left: 500, top: 600, width: 10, height: 10 })).toBeNull();
  });
});
