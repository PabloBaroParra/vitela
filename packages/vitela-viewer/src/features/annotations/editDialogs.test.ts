import { describe, expect, it } from 'vitest';
import type { PageSize } from '@vitela/core';
import { formatPt, parseMove, parseResize, proportional } from './editDialogs';

const LETTER: PageSize = { widthPt: 612, heightPt: 792, rotationDegrees: 0 };
const box = { left: 100, top: 200, width: 80, height: 40 };
const floor = { width: 4, height: 4 };

describe('formatPt', () => {
  it('shows at most two decimals, without trailing zeros', () => {
    expect(formatPt(100)).toBe('100');
    expect(formatPt(17.8)).toBe('17.8');
    expect(formatPt(33.33333)).toBe('33.33');
  });
});

describe('parseMove', () => {
  it('reads a typed top-left corner', () => {
    expect(parseMove('150', '180.5', box, LETTER)).toEqual({ left: 150, top: 180.5 });
  });

  it('accepts a decimal comma', () => {
    expect(parseMove('100,5', '200', box, LETTER)).toEqual({ left: 100.5, top: 200 });
  });

  it('refuses what is not a number, or a place that takes the box off the page', () => {
    expect(parseMove('', '200', box, LETTER)).toEqual({ error: 'Left must be a number of points.' });
    expect(parseMove('100', 'abc', box, LETTER)).toEqual({ error: 'Top must be a number of points.' });
    expect(parseMove('-1', '200', box, LETTER)).toEqual({ error: 'Left must be between 0 and 532 points.' });
    expect(parseMove('100', '753', box, LETTER)).toEqual({ error: 'Top must be between 0 and 752 points.' });
  });
});

describe('a length the user did not retype', () => {
  it('stays exactly what it was, not its two-decimal rounding', () => {
    const uneven = { left: 100.0049, top: 200, width: 80.004, height: 40 };
    expect(parseMove('100', '210', uneven, LETTER)).toEqual({ left: 100.0049, top: 210 });
    expect(parseResize('80', '50', uneven, floor, LETTER)).toEqual({ width: 80.004, height: 50 });
  });

  it('is not refused for being a rounding error under the floor', () => {
    const line = { left: 0, top: 0, width: 60, height: 17.8 };
    expect(parseResize('70', '17.8', line, { width: 16.01, height: 17.81 }, LETTER)).toEqual({ width: 70, height: 17.81 });
  });

  it('is not refused for already being off the page', () => {
    const off = { left: -5, top: 200, width: 80, height: 40 };
    expect(parseMove('-5', '210', off, LETTER)).toEqual({ left: -5, top: 210 });
  });
});

describe('parseResize', () => {
  it('reads a typed size', () => {
    expect(parseResize('120', '60', box, floor, LETTER)).toEqual({ width: 120, height: 60 });
  });

  it('refuses a size under the floor, past the page, or not a number', () => {
    expect(parseResize('x', '60', box, floor, LETTER)).toEqual({ error: 'Width must be a number of points.' });
    expect(parseResize('3', '60', box, floor, LETTER)).toEqual({ error: 'Width must be between 4 and 512 points.' });
    expect(parseResize('120', '600', box, floor, LETTER)).toEqual({ error: 'Height must be between 4 and 592 points.' });
    expect(parseResize('15', '60', box, { width: 16, height: 17.8 }, LETTER)).toEqual({
      error: 'Width must be between 16 and 512 points.',
    });
  });
});

describe('proportional', () => {
  it('gives the other side for the same proportions', () => {
    expect(proportional('160', 'width', box)).toBe('80');
    expect(proportional('10', 'height', box)).toBe('20');
  });

  it('leaves the other side alone while the typed one is not a size', () => {
    expect(proportional('', 'width', box)).toBeNull();
    expect(proportional('-3', 'width', box)).toBeNull();
  });
});
