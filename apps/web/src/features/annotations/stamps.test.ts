import { describe, expect, it } from 'vitest';
import { dataUrl, imageType, isStampFile, stampEdit, stampFileOf } from './stamps';

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13]);
const JPEG = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 0, 16]);
const file = (name: string, type = '') => new File([new Uint8Array([1])], name, { type });

describe('stamp images', () => {
  it('knows a PNG or a JPEG by its first bytes, whatever the file says it is', () => {
    expect(imageType(PNG)).toBe('image/png');
    expect(imageType(JPEG)).toBe('image/jpeg');
    expect(imageType(new TextEncoder().encode('%PDF-1.7'))).toBeNull();
    expect(imageType(new Uint8Array([0x89, 0x50]))).toBeNull();
    expect(imageType(new Uint8Array())).toBeNull();
  });

  it('turns the bytes into a data URL the layer can draw without any request', () => {
    expect(dataUrl(new Uint8Array([0x89, 0x50, 0x4e, 0x47]), 'image/png')).toBe('data:image/png;base64,iVBORw==');
    // Long enough to cross the chunking.
    const big = new Uint8Array(100_000).fill(0xff);
    const url = dataUrl(big, 'image/jpeg');
    expect(url.startsWith('data:image/jpeg;base64,////')).toBe(true);
    expect(atob(url.slice('data:image/jpeg;base64,'.length))).toHaveLength(100_000);
  });

  it('takes a PNG or JPEG file by its type or its name', () => {
    expect(isStampFile(file('logo.png', 'image/png'))).toBe(true);
    expect(isStampFile(file('photo.JPG'))).toBe(true);
    expect(isStampFile(file('photo.jpeg'))).toBe(true);
    expect(isStampFile(file('scan', 'image/jpeg'))).toBe(true);
    expect(isStampFile(file('icon.gif', 'image/gif'))).toBe(false);
    expect(isStampFile(file('a.pdf', 'application/pdf'))).toBe(false);
  });

  it('stamps the first image of a drop, unless a PDF came with it (that one opens)', () => {
    const logo = file('logo.png', 'image/png');
    expect(stampFileOf([file('notes.txt'), logo, file('b.jpg')])).toBe(logo);
    expect(stampFileOf([logo, file('a.pdf', 'application/pdf')])).toBeNull();
    expect(stampFileOf([file('notes.txt')])).toBeNull();
    expect(stampFileOf([])).toBeNull();
  });

  it('adds the stamp in the box the core gave it, with its bytes', () => {
    const rect = { left: 10, top: 20, width: 144, height: 72 };
    expect(stampEdit(2, rect, PNG)).toEqual({ kind: 'addStamp', page: 2, rect, image: PNG });
  });
});
