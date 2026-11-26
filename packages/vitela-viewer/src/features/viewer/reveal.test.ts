import { describe, expect, it } from 'vitest';
import { fittingMargins, revealScroll } from './reveal';

const VIEW = { left: 0, top: 1000, width: 800, height: 600 };

describe('revealScroll', () => {
  it('leaves a target that already shows where it is', () => {
    expect(revealScroll(VIEW, { left: 100, top: 1200, width: 50, height: 20 }, 16)).toBeNull();
  });

  it('centres a target below the viewport vertically, keeping the horizontal scroll', () => {
    expect(revealScroll(VIEW, { left: 100, top: 5000, width: 50, height: 20 }, 16)).toEqual({ x: 0, y: 5010 - 300 });
  });

  it('scrolls back up to a target above the viewport', () => {
    expect(revealScroll(VIEW, { left: 100, top: 400, width: 50, height: 20 }, 16)).toEqual({ x: 0, y: 410 - 300 });
  });

  it('scrolls sideways only when the target is off to the side', () => {
    expect(revealScroll(VIEW, { left: 1500, top: 1200, width: 100, height: 20 }, 16)).toEqual({ x: 1550 - 400, y: 1000 });
  });

  it('counts a target inside the margin as not showing', () => {
    expect(revealScroll(VIEW, { left: 100, top: 1590, width: 50, height: 20 }, 16)).not.toBeNull();
  });

  it('starts a target taller than the viewport at its top, less the margin', () => {
    expect(revealScroll(VIEW, { left: 100, top: 3000, width: 50, height: 900 }, 16)).toEqual({ x: 0, y: 2984 });
  });

  it('never asks for a negative scroll', () => {
    expect(revealScroll(VIEW, { left: 0, top: 10, width: 50, height: 20 }, 16)).toEqual({ x: 0, y: 0 });
  });
});

describe('revealScroll with a margin per axis', () => {
  it('keeps each margin to its own axis', () => {
    expect(revealScroll(VIEW, { left: 20, top: 1200, width: 50, height: 20 }, { x: 16, y: 80 })).toBeNull();
    expect(revealScroll(VIEW, { left: 20, top: 1040, width: 50, height: 20 }, { x: 16, y: 80 })).toEqual({ x: 0, y: 750 });
  });
});

describe('fittingMargins', () => {
  it('gives up the margin a target that fits leaves no room for', () => {
    const view = { width: 100, height: 50 };
    expect(fittingMargins(view, { width: 90, height: 40 }, { x: 16, y: 80 })).toEqual({ x: 5, y: 5 });
    expect(fittingMargins(view, { width: 300, height: 10 }, { x: 16, y: 15 })).toEqual({ x: 0, y: 15 });
  });
});
