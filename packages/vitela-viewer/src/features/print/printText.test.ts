import { describe, expect, it } from 'vitest';
import { printKey, printProgress } from './printText';

describe('printKey', () => {
  const press = (key: string, extra: Partial<KeyboardEvent> = {}) =>
    ({ key, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...extra }) as KeyboardEvent;

  it('is Ctrl+P, or ⌘+P on Apple platforms, with no other modifier', () => {
    expect(printKey(press('p', { ctrlKey: true }), false)).toBe(true);
    expect(printKey(press('P', { ctrlKey: true }), false)).toBe(true);
    expect(printKey(press('p', { metaKey: true }), true)).toBe(true);
    expect(printKey(press('p', { metaKey: true }), false)).toBe(false);
    expect(printKey(press('p', { ctrlKey: true }), true)).toBe(false);
    expect(printKey(press('p', { ctrlKey: true, shiftKey: true }), false)).toBe(false);
    expect(printKey(press('p', { ctrlKey: true, altKey: true }), false)).toBe(false);
    expect(printKey(press('p'), false)).toBe(false);
    expect(printKey(press('o', { ctrlKey: true }), false)).toBe(false);
  });
});

describe('printProgress', () => {
  it('counts the pages drawn so far', () => {
    expect(printProgress(2, 5)).toBe('Preparing page 2 of 5 for printing…');
  });
});
