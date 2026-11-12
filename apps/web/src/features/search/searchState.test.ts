import { describe, expect, it } from 'vitest';
import type { SearchHit } from '../../core/types';
import { IDLE, currentHit, hitsByPage, hitsOf, step, summary, type SearchState } from './searchState';

const hit = (pageIndex: number): SearchHit => ({ pageIndex, rects: new Float32Array([0, 0, 1, 1]) });
const found = (count: number, current = 0): SearchState => ({
  kind: 'found',
  query: 'beta',
  hits: Array.from({ length: count }, (_, index) => hit(index)),
  current,
});

describe('step', () => {
  it('moves forward and back between matches', () => {
    expect(currentHit(step(found(3), 1))?.pageIndex).toBe(1);
    expect(currentHit(step(found(3, 2), -1))?.pageIndex).toBe(1);
  });

  it('wraps past the last match to the first and back', () => {
    expect(currentHit(step(found(3, 2), 1))?.pageIndex).toBe(0);
    expect(currentHit(step(found(3, 0), -1))?.pageIndex).toBe(2);
  });

  it('makes a new state even onto the same match, so the viewer scrolls to it again', () => {
    const one = found(1);
    const next = step(one, 1);
    expect(next).not.toBe(one);
    expect(next).toEqual(one);
  });

  it('leaves states without matches as they are', () => {
    const none = found(0);
    expect(step(none, 1)).toBe(none);
    expect(step(IDLE, 1)).toBe(IDLE);
  });
});

describe('currentHit', () => {
  it('is the current match while found, and null otherwise', () => {
    expect(currentHit(found(2, 1))?.pageIndex).toBe(1);
    expect(currentHit(found(0))).toBeNull();
    expect(currentHit({ kind: 'searching', query: 'beta' })).toBeNull();
  });
});

describe('summary', () => {
  it('says where the current match is, or that there is none', () => {
    expect(summary(found(5, 1))).toBe('2 of 5');
    expect(summary(found(0))).toBe('No matches');
    expect(summary({ kind: 'searching', query: 'beta' })).toBe('Searching…');
    expect(summary({ kind: 'failed', query: 'beta', message: 'boom' })).toBe('Search failed: boom');
    expect(summary(IDLE)).toBe('');
  });
});

describe('hitsByPage', () => {
  it("groups the matches by page, keeping each match's rects", () => {
    const state: SearchState = { kind: 'found', query: 'beta', hits: [hit(0), hit(2), hit(2)], current: 0 };
    const pages = hitsByPage(hitsOf(state));
    expect([...pages.keys()]).toEqual([0, 2]);
    expect(pages.get(2)).toEqual([state.hits[1]!.rects, state.hits[2]!.rects]);
    expect(pages.get(2)![0]).toBe(state.hits[1]!.rects);
  });

  it('is empty unless found', () => {
    expect(hitsByPage(hitsOf(IDLE)).size).toBe(0);
  });

  it('keeps one array of matches per search while the current match moves', () => {
    const state = found(3);
    expect(hitsOf(step(state, 1))).toBe(hitsOf(state));
    expect(hitsOf({ kind: 'searching', query: 'x' })).toBe(hitsOf(IDLE));
  });
});
