import { describe, expect, it } from 'vitest';
import {
  extractRangeHint,
  extractStatus,
  extractedName,
  pageRunText,
  splitCancelled,
  splitDone,
  splitHint,
  splitPartFailed,
  splitProgress,
  splitWriteFailed,
} from './extractText';

describe('extractedName', () => {
  it('marks the stem as an extract, as Android and Windows suggest it', () => {
    expect(extractedName('report.pdf')).toBe('report-extract.pdf');
    expect(extractedName('Scan.PDF')).toBe('Scan-extract.pdf');
    expect(extractedName('.pdf')).toBe('document-extract.pdf');
  });
});

describe('pageRunText', () => {
  it('writes a run of pages in the grammar the dialog reads, one-based', () => {
    expect(pageRunText({ from: 1, count: 1 })).toBe('2');
    expect(pageRunText({ from: 1, count: 2 })).toBe('2-3');
  });
});

describe('the dialog hints', () => {
  it('count the pages, as the native shells do', () => {
    expect(extractRangeHint(1)).toBe('This document has 1 page.');
    expect(extractRangeHint(5)).toBe('This document has 5 pages.');
    expect(splitHint(5)).toBe('This document has 5 pages. Each cut starts a new file.');
  });
});

describe('extractStatus', () => {
  it('says how many pages went into which file', () => {
    expect(extractStatus({ how: 'downloaded', name: 'a-extract.pdf', fellBack: null }, 2, false)).toBe(
      'Downloaded a-extract.pdf: 2 pages extracted to a new PDF.',
    );
    expect(extractStatus({ how: 'delivered', name: 'a-extract.pdf' }, 1, false)).toBe(
      'Saved a-extract.pdf: 1 page extracted to a new PDF.',
    );
    expect(extractStatus({ how: 'cancelled' }, 1, false)).toBe('Extract cancelled. No file was written.');
  });

  it("adds the native shells' note for a signed source", () => {
    expect(extractStatus({ how: 'downloaded', name: 'a-extract.pdf', fellBack: null }, 1, true)).toBe(
      "Downloaded a-extract.pdf: 1 page extracted to a new PDF. The original document is signed, so the extracted PDF's signature no longer verifies.",
    );
  });
});

describe('the split status', () => {
  it('counts the parts as they go out and when they are done', () => {
    expect(splitProgress(2, 3)).toBe('Writing part 2 of 3…');
    expect(splitDone(3, false)).toBe('Split into 3 PDFs.');
    expect(splitDone(2, true)).toBe(
      'Split into 2 PDFs. The original document is signed, so the signature in each part no longer verifies.',
    );
  });

  it('says how far a stopped split got', () => {
    expect(splitCancelled(0)).toBe('Split cancelled. No file was written.');
    expect(splitCancelled(2)).toBe('Split cancelled. 2 written.');
    expect(splitPartFailed('a-part2.pdf', 'the writer broke', 1)).toBe(
      'Split stopped: a-part2.pdf could not be made: The writer broke. 1 written.',
    );
    expect(splitWriteFailed('a-part1.pdf', 0)).toBe('Split stopped: could not write a-part1.pdf. 0 written.');
  });
});
