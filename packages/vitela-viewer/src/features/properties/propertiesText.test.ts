import { describe, expect, it } from 'vitest';
import type { DocumentProperties } from '@vitela/core';
import { PROPERTY_FIELDS, draftOf, propertiesChange } from './propertiesText';

const none: DocumentProperties = { title: null, author: null, subject: null, keywords: null, creator: null, producer: null };

describe('the properties fields', () => {
  it("are the six text keys of /Info, in Android's order and words", () => {
    expect(PROPERTY_FIELDS.map(({ key, label }) => [key, label])).toEqual([
      ['title', 'Title'],
      ['author', 'Author'],
      ['subject', 'Subject'],
      ['keywords', 'Keywords'],
      ['creator', 'Creator'],
      ['producer', 'Producer'],
    ]);
  });
});

describe('draftOf', () => {
  it('shows an absent key as an empty box', () => {
    expect(draftOf({ ...none, title: 'Report' })).toEqual({
      title: 'Report',
      author: '',
      subject: '',
      keywords: '',
      creator: '',
      producer: '',
    });
  });
});

describe('propertiesChange', () => {
  it('is null when the boxes say what the document has', () => {
    const current = { ...none, title: 'Report' };
    expect(propertiesChange(current, draftOf(current))).toBeNull();
  });

  it('reads an emptied box as an absent key, never as an empty one', () => {
    const current = { ...none, title: 'Report', author: 'Ana' };
    expect(propertiesChange(current, { ...draftOf(current), author: '' })).toEqual({ ...none, title: 'Report' });
  });

  it('keeps what was typed as it was typed', () => {
    expect(propertiesChange(none, { ...draftOf(none), title: '  Annual report ' })).toEqual({ ...none, title: '  Annual report ' });
  });
});
