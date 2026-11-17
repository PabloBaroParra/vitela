// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { sha256, verifyAsset, type Digest } from './integrity.js';

const bytes = new TextEncoder().encode('abc').buffer as ArrayBuffer;
// sha256("abc"), the FIPS 180-2 test vector.
const ABC = 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad';

describe('verifyAsset', () => {
  it('accepts bytes whose sha256 matches (WebCrypto)', async () => {
    await expect(verifyAsset('pdfium.js', bytes, ABC, sha256)).resolves.toBeUndefined();
  });

  it('accepts an upper-case expected digest', async () => {
    await expect(verifyAsset('pdfium.js', bytes, ABC.toUpperCase(), sha256)).resolves.toBeUndefined();
  });

  it('rejects a mismatch, naming the file and both digests', async () => {
    const digest: Digest = async () => new Uint8Array(32).buffer;
    await expect(verifyAsset('pdfium.wasm', bytes, ABC, digest)).rejects.toThrow(
      `integrity check failed for pdfium.wasm: sha256 is ${'00'.repeat(32)}, expected ${ABC}`,
    );
  });

  it('rejects a file the package has no digest for', async () => {
    await expect(verifyAsset('pdfium.wasm', bytes, undefined)).rejects.toThrow(
      'integrity check failed for pdfium.wasm: this package has no digest for it',
    );
  });
});
