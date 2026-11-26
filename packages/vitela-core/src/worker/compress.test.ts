import { describe, expect, it } from 'vitest';
import {
  compressPresets,
  compressRefusal,
  compressedDocument,
  type CoreCompressRefusal,
  type CoreCompressedSave,
} from './compress.js';

function refusal(kind: string, message: string, freed: string[] = []): CoreCompressRefusal {
  return { kind, message, free: () => freed.push(`refusal:${kind}`) };
}

function saved(fields: Partial<CoreCompressedSave>, freed: string[] = []): CoreCompressedSave {
  return {
    beforeBytes: 1000,
    afterBytes: 400,
    savedBytes: 600,
    reduced: true,
    refusals: [],
    takeBytes: () => new Uint8Array([37, 80, 68, 70]),
    free: () => freed.push('saved'),
    ...fields,
  };
}

describe('compressPresets', () => {
  it('keeps the core order and drops a name this package does not know', () => {
    expect(compressPresets(['lossless', 'balanced', 'extreme', 'small'])).toEqual(['lossless', 'balanced', 'small']);
  });
});

describe('compressRefusal', () => {
  it('is null when the core refuses nothing (wasm-bindgen answers undefined)', () => {
    expect(compressRefusal(undefined)).toBeNull();
    expect(compressRefusal(null)).toBeNull();
  });

  it('copies a known refusal and frees the wasm object', () => {
    const freed: string[] = [];
    expect(compressRefusal(refusal('encryptedDocumentNotRewritable', 'this document is protected', freed))).toEqual({
      kind: 'encryptedDocumentNotRewritable',
      message: 'this document is protected',
    });
    expect(freed).toEqual(['refusal:encryptedDocumentNotRewritable']);
  });

  it('reads a kind it does not know as other, keeping the core message', () => {
    expect(compressRefusal(refusal('somethingNew', 'the core says why'))).toEqual({
      kind: 'other',
      message: 'the core says why',
    });
  });
});

describe('compressedDocument', () => {
  it('copies the report and the bytes, and frees every wasm object', () => {
    const freed: string[] = [];
    const document = compressedDocument(
      saved({ refusals: [refusal('signaturesWouldBeInvalidated', 'signed', freed)] }, freed),
    );
    expect(document).toEqual({
      bytes: new Uint8Array([37, 80, 68, 70]),
      beforeBytes: 1000,
      afterBytes: 400,
      savedBytes: 600,
      reduced: true,
      refusals: [{ kind: 'signaturesWouldBeInvalidated', message: 'signed' }],
    });
    expect(freed).toEqual(['refusal:signaturesWouldBeInvalidated', 'saved']);
  });

  it('frees the save even when reading it fails', () => {
    const freed: string[] = [];
    const broken = saved(
      {
        takeBytes: () => {
          throw new Error('out of memory');
        },
      },
      freed,
    );
    expect(() => compressedDocument(broken)).toThrow('out of memory');
    expect(freed).toEqual(['saved']);
  });
});
