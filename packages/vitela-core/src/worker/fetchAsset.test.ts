// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { fetchAsset, type FetchLike } from './fetchAsset.js';

const URL_ = 'https://cdn.example/v/pdfium.wasm';

describe('fetchAsset', () => {
  it("returns the response's bytes", async () => {
    const fetchImpl: FetchLike = async () => new Response(new Uint8Array([1, 2, 3]));
    expect(new Uint8Array(await fetchAsset(URL_, 1000, fetchImpl))).toEqual(new Uint8Array([1, 2, 3]));
  });

  it('names the URL and status on an HTTP error', async () => {
    const fetchImpl: FetchLike = async () => new Response('missing', { status: 404 });
    await expect(fetchAsset(URL_, 1000, fetchImpl)).rejects.toThrow(`${URL_}: HTTP 404`);
  });

  it('names the URL on a network or CORS failure', async () => {
    const fetchImpl: FetchLike = async () => {
      throw new TypeError('Failed to fetch');
    };
    await expect(fetchAsset(URL_, 1000, fetchImpl)).rejects.toThrow(
      `${URL_}: could not be fetched (network or CORS failure: Failed to fetch)`,
    );
  });

  it('aborts the fetch and names the URL when it outlives the timeout', async () => {
    let aborted = false;
    // A server that never answers; only the abort signal ends the request.
    const fetchImpl: FetchLike = (_url, { signal }) =>
      new Promise((_resolve, reject) => {
        signal.addEventListener('abort', () => {
          aborted = true;
          reject(signal.reason);
        });
      });
    await expect(fetchAsset(URL_, 20, fetchImpl)).rejects.toThrow(`${URL_}: timed out after 20 ms`);
    expect(aborted).toBe(true);
  });
});
