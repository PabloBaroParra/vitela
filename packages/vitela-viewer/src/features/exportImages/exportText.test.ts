import { describe, expect, it } from 'vitest';
import {
  DEFAULT_EXPORT_DPI,
  MAX_EXPORT_DPI,
  MIN_EXPORT_DPI,
  dpiRangeRefusal,
  exportDone,
  exportProgress,
  formatLabel,
  imageMediaType,
  openFailed,
  pageFailed,
  pageTooLarge,
  readDpi,
  writeFailed,
} from './exportText';

describe('readDpi', () => {
  it('reads a cleared box as the default, like the native resolution boxes', () => {
    expect(readDpi('')).toBe(DEFAULT_EXPORT_DPI);
    expect(readDpi('  ')).toBe(DEFAULT_EXPORT_DPI);
  });

  it('takes a whole number in range and refuses anything else rather than rounding into it', () => {
    expect(readDpi(' 300 ')).toBe(300);
    expect(readDpi(String(MIN_EXPORT_DPI))).toBe(72);
    expect(readDpi(String(MAX_EXPORT_DPI))).toBe(400);
    for (const typed of ['71', '401', '600', '150.5', '1e2', 'abc', '-150']) expect(readDpi(typed)).toBeNull();
  });
});

describe('export texts', () => {
  it('names each format and its media type', () => {
    expect(formatLabel('png')).toBe('PNG');
    expect(formatLabel('jpeg')).toBe('JPEG');
    expect(imageMediaType('png')).toBe('image/png');
    expect(imageMediaType('jpeg')).toBe('image/jpeg');
  });

  it('says what was refused in the native shells\' words', () => {
    expect(dpiRangeRefusal()).toBe('Choose a resolution between 72 and 400 DPI.');
    expect(pageTooLarge(3, 400)).toBe('Page 3 is too large to export at 400 DPI. Choose a lower resolution.');
  });

  it('tells the progress and the end, one page or many', () => {
    expect(exportProgress(7, 2, 5)).toBe('Exporting page 7 (2 of 5)…');
    expect(exportDone(1, 'png')).toBe('Exported 1 page as PNG.');
    expect(exportDone(3, 'jpeg')).toBe('Exported 3 pages as JPEG.');
  });

  it('counts what was written when an export stops part-way', () => {
    expect(pageFailed(4, 'page could not be rendered', 3)).toBe(
      'Page 4 could not be exported: Page could not be rendered. 3 written.',
    );
    expect(writeFailed('a-2.png', 1)).toBe('Could not write a-2.png. 1 written.');
    expect(openFailed('save failed')).toBe('Could not export: Save failed. No file was written.');
  });
});
