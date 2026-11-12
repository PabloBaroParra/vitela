import { describe, expect, it } from 'vitest';
import { revealScroll } from './reveal';

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
