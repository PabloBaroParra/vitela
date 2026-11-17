import { describe, expect, it } from 'vitest';
import { gridKeyAction, type KeyPress } from './gridKeys';

const press = (key: string, mods: Partial<KeyPress> = {}): KeyPress => ({
  key,
  shiftKey: false,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  ...mods,
});
/** Ten pages, four a row, the focus on page 6 (index 5). */
const at = (key: KeyPress) => gridKeyAction(key, { focus: 5, columns: 4, pageCount: 10 });

describe('gridKeyAction', () => {
  it('moves the focus by a card or a row, selecting it', () => {
    expect(at(press('ArrowRight'))).toEqual({ kind: 'focus', index: 6, extend: false });
    expect(at(press('ArrowLeft'))).toEqual({ kind: 'focus', index: 4, extend: false });
    expect(at(press('ArrowDown'))).toEqual({ kind: 'focus', index: 9, extend: false });
    expect(at(press('ArrowUp'))).toEqual({ kind: 'focus', index: 1, extend: false });
    expect(at(press('Home'))).toEqual({ kind: 'focus', index: 0, extend: false });
    expect(at(press('End'))).toEqual({ kind: 'focus', index: 9, extend: false });
  });

  it('extends the selection with Shift', () => {
    expect(at(press('ArrowRight', { shiftKey: true }))).toEqual({ kind: 'focus', index: 6, extend: true });
  });

  it('stays inside the document', () => {
    expect(gridKeyAction(press('ArrowRight'), { focus: 9, columns: 4, pageCount: 10 })).toEqual({
      kind: 'focus',
      index: 9,
      extend: false,
    });
    expect(gridKeyAction(press('ArrowDown'), { focus: 7, columns: 4, pageCount: 10 })).toEqual({
      kind: 'focus',
      index: 9,
      extend: false,
    });
    expect(gridKeyAction(press('ArrowUp'), { focus: 2, columns: 4, pageCount: 10 })).toEqual({
      kind: 'focus',
      index: 0,
      extend: false,
    });
  });

  it('moves the selected pages with Ctrl+Shift (or Cmd+Shift) and an arrow', () => {
    expect(at(press('ArrowLeft', { ctrlKey: true, shiftKey: true }))).toEqual({ kind: 'move', delta: -1 });
    expect(at(press('ArrowRight', { metaKey: true, shiftKey: true }))).toEqual({ kind: 'move', delta: 1 });
    expect(at(press('ArrowUp', { ctrlKey: true, shiftKey: true }))).toEqual({ kind: 'move', delta: -4 });
    expect(at(press('ArrowDown', { ctrlKey: true, shiftKey: true }))).toEqual({ kind: 'move', delta: 4 });
  });

  it('deletes with Delete or Backspace', () => {
    expect(at(press('Delete'))).toEqual({ kind: 'delete' });
    expect(at(press('Backspace'))).toEqual({ kind: 'delete' });
  });

  it('leaves every other key, and Alt or Ctrl-only chords, to the browser', () => {
    expect(at(press('a'))).toBeNull();
    expect(at(press('ArrowLeft', { altKey: true }))).toBeNull();
    expect(at(press('ArrowLeft', { ctrlKey: true }))).toBeNull();
  });

  it('starts from the first page when nothing is focused', () => {
    expect(gridKeyAction(press('ArrowRight'), { focus: null, columns: 4, pageCount: 10 })).toEqual({
      kind: 'focus',
      index: 0,
      extend: false,
    });
    expect(gridKeyAction(press('Delete'), { focus: null, columns: 4, pageCount: 10 })).toBeNull();
  });

  it('does nothing with no pages', () => {
    expect(gridKeyAction(press('ArrowRight'), { focus: null, columns: 4, pageCount: 0 })).toBeNull();
  });
});
