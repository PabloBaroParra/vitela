import { describe, expect, it } from 'vitest';
import {
  compressedName,
  compressStatus,
  humanSize,
  percentSmaller,
  presetText,
  sentence,
  type CompressReport,
} from './compressText';

const report = (fields: Partial<CompressReport> = {}): CompressReport => ({
  beforeBytes: 2_400_000,
  afterBytes: 1_200_000,
  savedBytes: 1_200_000,
  reduced: true,
  refusals: [],
  ...fields,
});

describe('humanSize', () => {
  it('counts in powers of ten, as the other shells do', () => {
    expect(humanSize(1)).toBe('1 byte');
    expect(humanSize(999)).toBe('999 bytes');
    expect(humanSize(1_000)).toBe('1 kB');
    expect(humanSize(237_961)).toBe('238 kB');
    expect(humanSize(1_000_000)).toBe('1.0 MB');
    expect(humanSize(2_450_000)).toBe('2.5 MB');
  });
});

describe('percentSmaller', () => {
  it('truncates, so the number shown is one the file reached', () => {
    expect(percentSmaller(report({ beforeBytes: 1000, savedBytes: 497 }))).toBe(49);
    expect(percentSmaller(report({ beforeBytes: 0, savedBytes: 0 }))).toBe(0);
  });
});

describe('compressedName', () => {
  it('names the copy after the document, like Android', () => {
    expect(compressedName('report.pdf')).toBe('report-compressed.pdf');
    expect(compressedName('Scan.PDF')).toBe('Scan-compressed.pdf');
    expect(compressedName('notes')).toBe('notes-compressed.pdf');
    expect(compressedName('  ')).toBe('document-compressed.pdf');
  });
});

describe('sentence', () => {
  it("turns the core's clause into a sentence", () => {
    expect(sentence('this cannot be compressed')).toBe('This cannot be compressed.');
    expect(sentence('Already one.')).toBe('Already one.');
  });
});

describe('presetText', () => {
  it('names every preset with the wording the other shells use', () => {
    expect(presetText('lossless').name).toBe('Lossless');
    expect(presetText('balanced').detail).toMatch(/150 DPI/);
    expect(presetText('small').detail).toMatch(/96 DPI/);
  });
});

describe('compressStatus', () => {
  it('reports a reduction and where the copy went', () => {
    expect(compressStatus({ kind: 'reduced', report: report(), outcome: { how: 'downloaded', name: 'a-compressed.pdf', fellBack: null } })).toBe(
      '2.4 MB uncompressed → 1.2 MB compressed (50% smaller). Downloaded a-compressed.pdf.',
    );
    expect(compressStatus({ kind: 'reduced', report: report(), outcome: { how: 'delivered', name: 'a-compressed.pdf' } })).toBe(
      '2.4 MB uncompressed → 1.2 MB compressed (50% smaller). Saved a-compressed.pdf.',
    );
  });

  it('says nothing was written when nothing smaller came out', () => {
    expect(compressStatus({ kind: 'noGain', report: report({ afterBytes: 2_400_000, savedBytes: 0, reduced: false }) })).toBe(
      'Nothing to gain: this document compresses to the same 2.4 MB an uncompressed save writes, so no file was written.',
    );
  });

  it('adds what could not be done, in the core words', () => {
    const refused = report({
      afterBytes: 2_400_000,
      savedBytes: 0,
      reduced: false,
      refusals: [{ kind: 'other', message: 'one thing' }, { kind: 'other', message: 'another' }],
    });
    expect(compressStatus({ kind: 'noGain', report: refused })).toBe(
      'Nothing to gain: this document compresses to the same 2.4 MB an uncompressed save writes, so no file was written. Not everything could be done: one thing, another.',
    );
  });
});
