// Checks an asset fetched from a custom base URL (a CDN) against the sha256
// of the copy this package version ships, before it is compiled or evaluated.

/** Hashes bytes; injectable so the check is testable without WebCrypto. */
export type Digest = (bytes: ArrayBuffer) => Promise<ArrayBuffer>;

/** SHA-256 via WebCrypto (only in secure contexts: https or localhost). */
export const sha256: Digest = (bytes) => {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) {
    return Promise.reject(
      new Error('cannot verify the engine assets: WebCrypto is unavailable (the page is not a secure context)'),
    );
  }
  return subtle.digest('SHA-256', bytes);
};

/**
 * Resolves when `bytes` hash to `expectedHex`; otherwise rejects naming the
 * file, so a tampered or mismatched CDN copy is never run.
 */
export async function verifyAsset(
  fileName: string,
  bytes: ArrayBuffer,
  expectedHex: string | undefined,
  digest: Digest = sha256,
): Promise<void> {
  if (expectedHex === undefined) {
    throw new Error(`integrity check failed for ${fileName}: this package has no digest for it`);
  }
  const actualHex = toHex(await digest(bytes));
  if (actualHex !== expectedHex.toLowerCase()) {
    throw new Error(
      `integrity check failed for ${fileName}: sha256 is ${actualHex}, expected ${expectedHex} ` +
        '(assetBaseUrl must serve the files from this exact @vitela/core version)',
    );
  }
}

function toHex(digest: ArrayBuffer): string {
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
}
