import { describe, expect, it } from 'vitest';
import { editKey, NUDGE_PT, NUDGE_LARGE_PT } from './editKeys';

const press = (key: string, modifiers: Partial<{ shiftKey: boolean; altKey: boolean; ctrlKey: boolean; metaKey: boolean }> = {}) => ({
  key,
  shiftKey: false,
  altKey: false,
  ctrlKey: false,
  metaKey: false,
  ...modifiers,
});

describe('editKey', () => {
  it('moves by a point with an arrow key, and by the native Nudge step with Shift', () => {
    expect(NUDGE_PT).toBe(1);
    expect(NUDGE_LARGE_PT).toBe(12);
    expect(editKey(press('ArrowRight'))).toEqual({ kind: 'move', dx: 1, dy: 0 });
    expect(editKey(press('ArrowLeft'))).toEqual({ kind: 'move', dx: -1, dy: 0 });
    expect(editKey(press('ArrowUp', { shiftKey: true }))).toEqual({ kind: 'move', dx: 0, dy: -12 });
    expect(editKey(press('ArrowDown', { shiftKey: true }))).toEqual({ kind: 'move', dx: 0, dy: 12 });
  });

  it('resizes with Alt: right and down grow, left and up shrink', () => {
    expect(editKey(press('ArrowRight', { altKey: true }))).toEqual({ kind: 'resize', dw: 1, dh: 0 });
    expect(editKey(press('ArrowUp', { altKey: true }))).toEqual({ kind: 'resize', dw: 0, dh: -1 });
    expect(editKey(press('ArrowDown', { altKey: true, shiftKey: true }))).toEqual({ kind: 'resize', dw: 0, dh: 12 });
  });

  it('removes with Delete or Backspace and lets go with Escape', () => {
    expect(editKey(press('Delete'))).toEqual({ kind: 'remove' });
    expect(editKey(press('Backspace'))).toEqual({ kind: 'remove' });
    expect(editKey(press('Escape'))).toEqual({ kind: 'deselect' });
  });

  it('leaves Ctrl/⌘ combinations and other keys alone', () => {
    expect(editKey(press('ArrowRight', { ctrlKey: true }))).toBeNull();
    expect(editKey(press('ArrowRight', { metaKey: true }))).toBeNull();
    expect(editKey(press('Delete', { shiftKey: true }))).toBeNull();
    expect(editKey(press('a'))).toBeNull();
  });
});
