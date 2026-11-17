// Fetches one engine asset with a deadline. Every failure names the URL, so
// the client's `fatal` says which file (and host) did not load.

export type FetchLike = (url: string, init: { signal: AbortSignal }) => Promise<Response>;

/** The asset's bytes, or an error naming `url` (HTTP status, network/CORS failure or timeout). */
export async function fetchAsset(url: string, timeoutMs: number, fetchImpl: FetchLike = fetch): Promise<ArrayBuffer> {
  const signal = AbortSignal.timeout(timeoutMs);
  try {
    const response = await fetchImpl(url, { signal });
    if (!response.ok) throw new AssetHttpError(`${url}: HTTP ${response.status}`);
    // The body read stays under the same deadline.
    return await response.arrayBuffer();
  } catch (error) {
    if (error instanceof AssetHttpError) throw error;
    if (signal.aborted) throw new Error(`${url}: timed out after ${timeoutMs} ms`);
    // Browsers report network and CORS failures alike, as a bare TypeError.
    const reason = error instanceof Error ? error.message : String(error);
    throw new Error(`${url}: could not be fetched (network or CORS failure: ${reason})`);
  }
}

class AssetHttpError extends Error {}
