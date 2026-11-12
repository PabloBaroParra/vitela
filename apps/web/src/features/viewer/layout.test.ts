import { describe, expect, it } from 'vitest';
import type { PageSize } from '../../core/types';
import { CSS_PX_PER_PT, PAGE_GAP, VIEWER_PADDING, anchorAt, layoutPages, pageAt, positionOf, visiblePages } from './layout';

const letter: PageSize = { widthPt: 612, heightPt: 792, rotationDegrees: 0 };
// A portrait page turned a quarter: the core already reports it landscape.
const turned: PageSize = { widthPt: 792, heightPt: 612, rotationDegrees: 90 };

describe('layoutPages', () => {
  it('stacks pages top to bottom at CSS size times zoom, with gaps and padding', () => {
    const layout = layoutPages([letter, letter], 1, 2000);
    const [first, second] = layout.slots;
    expect(first).toEqual({
      top: VIEWER_PADDING,
      left: (2000 - 612 * CSS_PX_PER_PT) / 2,
      width: 612 * CSS_PX_PER_PT,
      height: 792 * CSS_PX_PER_PT,
    });
    expect(second!.top).toBe(VIEWER_PADDING + 792 * CSS_PX_PER_PT + PAGE_GAP);
    expect(layout.height).toBe(second!.top + second!.height + VIEWER_PADDING);
    expect(layout.width).toBe(2000);
  });

  it('lays a rotated page out with the size the core reports', () => {
    const [slot] = layoutPages([turned], 1, 2000).slots;
    expect(slot!.width).toBeGreaterThan(slot!.height);
    expect(slot!.width).toBe(792 * CSS_PX_PER_PT);
  });

  it('scales every page with the zoom', () => {
    const [slot] = layoutPages([letter], 2, 4000).slots;
    expect([slot!.width, slot!.height]).toEqual([612 * CSS_PX_PER_PT * 2, 792 * CSS_PX_PER_PT * 2]);
  });

  it('widens the content past the viewport when the widest page does not fit', () => {
    const layout = layoutPages([letter, turned], 1, 300);
    expect(layout.width).toBe(792 * CSS_PX_PER_PT + 2 * VIEWER_PADDING);
    expect(layout.slots[1]!.left).toBe(VIEWER_PADDING);
  });
});

describe('visiblePages', () => {
  const layout = layoutPages(Array<PageSize>(50).fill(letter), 1, 1000);
  const pitch = 792 * CSS_PX_PER_PT + PAGE_GAP;

  it('returns only the pages that intersect the viewport', () => {
    expect(visiblePages(layout, 0, 700, 0)).toEqual({ first: 0, last: 0 });
    expect(visiblePages(layout, pitch * 10 + VIEWER_PADDING, 700, 0)).toEqual({ first: 10, last: 10 });
    expect(visiblePages(layout, pitch * 10 - 50, 700, 0)).toEqual({ first: 9, last: 10 });
  });

  it('extends the range by the overscan on both sides', () => {
    expect(visiblePages(layout, pitch * 10 + VIEWER_PADDING, 700, pitch)).toEqual({ first: 9, last: 11 });
  });

  it('clamps at both ends of the document', () => {
    expect(visiblePages(layout, layout.height - 700, 700, 5000)).toMatchObject({ last: 49 });
    expect(visiblePages(layout, 0, 700, 5000)).toMatchObject({ first: 0 });
  });

  it('is empty for a document with no pages', () => {
    expect(visiblePages(layoutPages([], 1, 1000), 0, 700, 0)).toBeNull();
  });
});

describe('pageAt', () => {
  it('finds the page under a content y, counting a gap as the page above it', () => {
    const layout = layoutPages([letter, letter, letter], 1, 1000);
    expect(pageAt(layout, 0)).toBe(0);
    expect(pageAt(layout, layout.slots[1]!.top + 1)).toBe(1);
    expect(pageAt(layout, layout.slots[1]!.top - 1)).toBe(0);
    expect(pageAt(layout, layout.height + 100)).toBe(2);
  });

  it('has no page in an empty document', () => {
    expect(pageAt(layoutPages([], 1, 1000), 0)).toBeNull();
  });
});

describe('anchorAt / positionOf', () => {
  it('maps a content point to a page-relative anchor and back', () => {
    const layout = layoutPages([letter, letter], 1, 1000);
    const slot = layout.slots[1]!;
    const point = { x: slot.left + slot.width / 4, y: slot.top + slot.height / 2 };
    const anchor = anchorAt(layout, point);
    expect(anchor).toEqual({ page: 1, x: 0.25, y: 0.5 });
    expect(positionOf(layout, anchor!)).toEqual(point);
  });

  it('puts the same page point at the matching place of a zoomed layout', () => {
    const before = layoutPages([letter, letter], 1, 1000);
    const after = layoutPages([letter, letter], 2, 1000);
    const anchor = anchorAt(before, { x: before.slots[1]!.left, y: before.slots[1]!.top })!;
    expect(positionOf(after, anchor)).toEqual({ x: after.slots[1]!.left, y: after.slots[1]!.top });
  });

  it('has no anchor in an empty document', () => {
    expect(anchorAt(layoutPages([], 1, 1000), { x: 0, y: 0 })).toBeNull();
  });
});
