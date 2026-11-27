import { describe, expect, it } from 'vitest';
import type { TextBlock } from '@vitela/core';
import { INSERT_TEXT_SIZE_PT, insertedRect, insertionOf, paragraphKey, paragraphOf, paragraphRetypeOf } from './textBlocks';

const line = (runIds: number[], top: number, text: string) => ({ runIds, rect: { left: 72, top, width: 200, height: 12 }, text });

const heading: TextBlock = {
  runIds: [1, 2],
  rect: { left: 72, top: 80, width: 300, height: 12 },
  text: 'Hello world',
  lines: [line([1, 2], 80, 'Hello world')],
  leadingPt: null,
  alignment: 'left',
};
const paragraph: TextBlock = {
  runIds: [3, 4, 5],
  rect: { left: 72, top: 130, width: 200, height: 40 },
  text: 'First line\nSecond line\nThird',
  lines: [line([3], 130, 'First line'), line([4], 144, 'Second line'), line([5], 158, 'Third')],
  leadingPt: 14,
  alignment: 'left',
};

describe('paragraphOf', () => {
  it('finds the paragraph a run belongs to', () => {
    expect(paragraphOf([heading, paragraph], 4)).toBe(paragraph);
  });

  it('is null for a run on a one-line block or in no block', () => {
    expect(paragraphOf([heading, paragraph], 2)).toBeNull();
    expect(paragraphOf([heading, paragraph], 99)).toBeNull();
  });
});

describe('paragraphKey', () => {
  it('commits on Ctrl or Cmd+Enter, cancels on Escape, and leaves a plain Enter to the field', () => {
    expect(paragraphKey('Enter', true)).toBe('commit');
    expect(paragraphKey('Enter', false)).toBe('type');
    expect(paragraphKey('Escape', false)).toBe('cancel');
    expect(paragraphKey('a', true)).toBe('type');
  });
});

describe('paragraphRetypeOf', () => {
  it('retypes the whole block by its runs, line breaks kept', () => {
    expect(paragraphRetypeOf(2, paragraph, 'New first\nSecond line\nThird')).toEqual({
      kind: 'retypeTextBlock',
      page: 2,
      runIds: [3, 4, 5],
      text: 'New first\nSecond line\nThird',
    });
  });

  it('asks for nothing when the text did not change', () => {
    expect(paragraphRetypeOf(2, paragraph, paragraph.text)).toBeNull();
  });

  it('normalises Windows line breaks a paste may bring', () => {
    expect(paragraphRetypeOf(0, paragraph, 'First line\r\nSecond line\r\nThird')).toBeNull();
  });
});

describe('insertionOf', () => {
  it('inserts the typed line at the point in the default size', () => {
    expect(insertionOf(1, { x: 50, y: 60 }, 'Note')).toEqual({ kind: 'insertText', page: 1, at: { x: 50, y: 60 }, text: 'Note', sizePt: INSERT_TEXT_SIZE_PT });
  });

  it('asks for nothing when nothing was typed', () => {
    expect(insertionOf(1, { x: 50, y: 60 }, '')).toBeNull();
    expect(insertionOf(1, { x: 50, y: 60 }, '   ')).toBeNull();
  });
});

describe('insertedRect', () => {
  it('hangs the line from the point on an upright page', () => {
    const rect = insertedRect({ x: 50, y: 60 }, 'abcd', 14, 0);
    expect(rect).toMatchObject({ left: 50, top: 60, height: 14 });
    expect(rect.width).toBeGreaterThan(0);
  });

  it('turns the box with the page, the point staying its top-left corner in the page’s own frame', () => {
    const upright = insertedRect({ x: 50, y: 60 }, 'abcd', 10, 0);
    expect(insertedRect({ x: 50, y: 60 }, 'abcd', 10, 90)).toEqual({ left: 40, top: 60, width: 10, height: upright.width });
    expect(insertedRect({ x: 50, y: 60 }, 'abcd', 10, 180)).toEqual({ left: 50 - upright.width, top: 50, width: upright.width, height: 10 });
    expect(insertedRect({ x: 50, y: 60 }, 'abcd', 10, 270)).toEqual({ left: 50, top: 60 - upright.width, width: 10, height: upright.width });
  });
});
