import { describe, expect, it } from 'vitest';
import type { PageContent } from '@vitela/core';
import { pressWaits, settlePress, type PendingPress } from './pendingPress';

const content: PageContent = { runs: [], images: [], paths: [] };
const press: PendingPress = { pageIndex: 2, key: 'k2', point: { x: 80, y: 80 } };

describe('pressWaits', () => {
  it('waits for a page not read yet, and not for one read or failed', () => {
    expect(pressWaits(undefined)).toBe(true);
    expect(pressWaits({ status: 'loading' })).toBe(true);
    expect(pressWaits({ status: 'ready', content })).toBe(false);
    expect(pressWaits({ status: 'failed', message: 'Broken.' })).toBe(false);
  });
});

describe('settlePress', () => {
  it('keeps waiting while its page is still being read', () => {
    expect(settlePress(press, 'k2', { status: 'loading' }, true)).toEqual({ kind: 'wait' });
    expect(settlePress(press, 'k2', undefined, true)).toEqual({ kind: 'wait' });
  });

  it('resolves against the read once it lands', () => {
    expect(settlePress(press, 'k2', { status: 'ready', content }, true)).toEqual({ kind: 'resolve', content });
  });

  it('drops the press when the read fails', () => {
    expect(settlePress(press, 'k2', { status: 'failed', message: 'Broken.' }, true)).toEqual({ kind: 'drop' });
  });

  it('drops the press when its position shows another page or read', () => {
    expect(settlePress(press, 'k3', { status: 'ready', content }, true)).toEqual({ kind: 'drop' });
    expect(settlePress(press, undefined, undefined, true)).toEqual({ kind: 'drop' });
  });

  it('drops the press when its page is no longer read', () => {
    expect(settlePress(press, 'k2', undefined, false)).toEqual({ kind: 'drop' });
  });
});
