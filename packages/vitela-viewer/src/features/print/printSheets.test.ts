import { describe, expect, it } from 'vitest';
import { PRINT_DPI, PRINT_MIN_DPI, PRINT_PIXEL_BUDGET, pngSize, printDpi, printStyles, sheetOf, writePrintDocument } from './printSheets';

/** The first 24 bytes of a PNG: signature, IHDR length and type, width, height. */
function pngHeader(width: number, height: number): Uint8Array {
  const bytes = new Uint8Array(24);
  bytes.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52]);
  const view = new DataView(bytes.buffer);
  view.setUint32(16, width);
  view.setUint32(20, height);
  return bytes;
}

describe('pngSize', () => {
  it("reads a PNG's width and height from its header, and nothing else", () => {
    expect(pngSize(pngHeader(1275, 1650))).toEqual({ width: 1275, height: 1650 });
    expect(pngSize(new Uint8Array([0xff, 0xd8, 0xff]))).toBeNull();
    const notPng = pngHeader(10, 10);
    notPng[1] = 0;
    expect(pngSize(notPng)).toBeNull();
  });
});

describe('sheetOf', () => {
  it('is the page in points, from the pixels and the DPI they were drawn at', () => {
    expect(PRINT_DPI).toBe(150);
    // US Letter at 150 DPI, upright and turned a quarter.
    expect(sheetOf('blob:a', { width: 1275, height: 1650 }, 150)).toEqual({ url: 'blob:a', widthPt: 612, heightPt: 792 });
    expect(sheetOf('blob:b', { width: 1650, height: 1275 }, 150)).toEqual({ url: 'blob:b', widthPt: 792, heightPt: 612 });
  });
});

describe('printDpi', () => {
  const letter = { widthPt: 612, heightPt: 792 };
  const letters = (count: number) => Array.from({ length: count }, () => letter);
  /** The pixels every page drawn at `dpi` adds up to. */
  const pixels = (count: number, dpi: number) => count * ((612 / 72) * dpi) * ((792 / 72) * dpi);

  it('prints a short document at the full print resolution', () => {
    expect(PRINT_DPI).toBe(150);
    expect(printDpi(letters(1), 1)).toBe(150);
    expect(printDpi(letters(60), 60)).toBe(150);
  });

  it('lowers the resolution of a long document so every page together stays within the budget', () => {
    expect(PRINT_PIXEL_BUDGET).toBe(128_000_000);
    const dpi = printDpi(letters(100), 100);
    expect(dpi).toBeLessThan(150);
    expect(dpi).toBeGreaterThan(PRINT_MIN_DPI);
    expect(pixels(100, dpi)).toBeLessThanOrEqual(PRINT_PIXEL_BUDGET);
    expect(pixels(100, dpi + 1)).toBeGreaterThan(PRINT_PIXEL_BUDGET);
  });

  it('never goes below the floor, however long the document', () => {
    expect(PRINT_MIN_DPI).toBe(96);
    expect(printDpi(letters(2000), 2000)).toBe(96);
  });

  it('counts pages it has no size for as the average of the ones it has, or Letter', () => {
    expect(printDpi(letters(2), 100)).toBe(printDpi(letters(100), 100));
    expect(printDpi([], 100)).toBe(printDpi(letters(100), 100));
    // A size list longer than the snapshot counts only the snapshot's pages.
    expect(printDpi(letters(100), 1)).toBe(150);
  });
});

describe('printStyles', () => {
  it('names one page size per distinct sheet, so each page prints on a sheet of its own size and orientation', () => {
    const css = printStyles([
      { url: 'a', widthPt: 612, heightPt: 792 },
      { url: 'b', widthPt: 792, heightPt: 612 },
      { url: 'c', widthPt: 612, heightPt: 792 },
    ]);
    expect(css).toContain('@page { margin: 0; }');
    expect(css).toContain('@page vitela-sheet-0 { size: 612pt 792pt; margin: 0; }');
    expect(css).toContain('@page vitela-sheet-1 { size: 792pt 612pt; margin: 0; }');
    expect(css).not.toContain('vitela-sheet-2');
    expect(css).toContain('.sheet--0 { page: vitela-sheet-0; width: 612pt; height: 792pt; }');
  });
});

describe('writePrintDocument', () => {
  it('writes one sheet per page, in order, titled after the document, with nothing of the host in it', () => {
    const doc = window.document.implementation.createHTMLDocument('');
    const sheets = [
      { url: 'blob:one', widthPt: 612, heightPt: 792 },
      { url: 'blob:two', widthPt: 792, heightPt: 612 },
    ];
    const images = writePrintDocument(doc, 'report.pdf', sheets);
    expect(doc.title).toBe('report.pdf');
    expect(doc.head.querySelectorAll('style')).toHaveLength(1);
    expect(doc.head.querySelector('link')).toBeNull();
    const shown = [...doc.body.querySelectorAll('.sheet')];
    expect(shown.map((sheet) => sheet.className)).toEqual(['sheet sheet--0', 'sheet sheet--1']);
    expect(images.map((image) => [image.getAttribute('src'), image.alt])).toEqual([
      ['blob:one', 'Page 1'],
      ['blob:two', 'Page 2'],
    ]);
    // Writing again replaces, never appends.
    writePrintDocument(doc, 'report.pdf', sheets.slice(0, 1));
    expect(doc.body.querySelectorAll('.sheet')).toHaveLength(1);
    expect(doc.head.querySelectorAll('style')).toHaveLength(1);
  });
});
