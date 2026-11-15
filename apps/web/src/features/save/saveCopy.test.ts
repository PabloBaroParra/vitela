import { describe, expect, it } from 'vitest';
import { copyName, saveCopy } from './saveCopy';

const bytes = new Uint8Array([37, 80, 68, 70]);

function downloads() {
  const calls: [Uint8Array, string][] = [];
  return { calls, download: (data: Uint8Array, name: string) => void calls.push([data, name]) };
}

describe('copyName', () => {
  it('puts (copy) before the extension', () => {
    expect(copyName('report.pdf')).toBe('report (copy).pdf');
    expect(copyName('Report.PDF')).toBe('Report (copy).pdf');
  });

  it('strips only a real ".pdf" extension', () => {
    expect(copyName('mypdf')).toBe('mypdf (copy).pdf');
    expect(copyName('report.xpdf')).toBe('report.xpdf (copy).pdf');
  });

  it('gives a name without the extension one', () => {
    expect(copyName('report')).toBe('report (copy).pdf');
  });
});

describe('saveCopy', () => {
  it('downloads the copy under its own name, never writing to any file', () => {
    const { calls, download } = downloads();
    expect(saveCopy({ bytes, name: 'a.pdf', download })).toEqual({ how: 'copied', name: 'a (copy).pdf' });
    expect(calls).toEqual([[bytes, 'a (copy).pdf']]);
  });
});
