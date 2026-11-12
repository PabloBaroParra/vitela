import { describe, expect, it } from 'vitest';
import { carriesFiles, droppedFile } from './droppedFile';

const file = (name: string, type = '') => new File([new Uint8Array([1])], name, { type });

describe('droppedFile', () => {
  it('prefers a PDF among several dropped files, by type or by extension', () => {
    const notes = file('notes.txt', 'text/plain');
    const typed = file('scan', 'application/pdf');
    const named = file('Report.PDF');
    expect(droppedFile([notes, typed])).toBe(typed);
    expect(droppedFile([notes, named])).toBe(named);
  });

  it('falls back to the first file, so the engine explains why it is not a PDF', () => {
    const notes = file('notes.txt', 'text/plain');
    expect(droppedFile([notes, file('b.png', 'image/png')])).toBe(notes);
  });

  it('is null when nothing was dropped', () => {
    expect(droppedFile([])).toBeNull();
  });
});

describe('carriesFiles', () => {
  it('tells a file drag from a text or link drag', () => {
    expect(carriesFiles(['Files'])).toBe(true);
    expect(carriesFiles(['text/plain', 'text/uri-list'])).toBe(false);
    expect(carriesFiles(undefined)).toBe(false);
  });
});
