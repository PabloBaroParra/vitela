// What a host page can hand the editor to open, turned into the named source
// of bytes the workspace opens from. Nothing is read until the editor opens
// it; a URL is fetched once, however many times its bytes are read (a password
// retry reads them again).

import type { PdfSource } from '../features/document/useDocument';

/** A PDF to open: a `File`, its bytes, or a URL to fetch them from (relative URLs resolve against the page). */
export type ViewerSource = File | Uint8Array | string | URL;

/** The name a source gets when neither it nor the host names it. */
export const DEFAULT_NAME = 'document.pdf';

export function toPdfSource(source: ViewerSource, name?: string): PdfSource {
  if (source instanceof Uint8Array) {
    // A copy per read: the engine takes the bytes it is given by transfer, and
    // the host's array must stay usable.
    return { name: name ?? DEFAULT_NAME, arrayBuffer: async () => source.slice().buffer };
  }
  if (typeof source === 'string' || source instanceof URL) return urlSource(source, name);
  return name === undefined || name === source.name ? source : { name, arrayBuffer: () => source.arrayBuffer() };
}

function urlSource(url: string | URL, name: string | undefined): PdfSource {
  let fetched: Promise<ArrayBuffer> | undefined;
  const load = async () => {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`HTTP ${response.status}${response.statusText ? ` ${response.statusText}` : ''}`);
    return response.arrayBuffer();
  };
  return {
    name: name ?? nameOf(url),
    arrayBuffer: async () => {
      // A failed fetch is tried again on the next read; a good one is kept.
      fetched ??= load().catch((error: unknown) => {
        fetched = undefined;
        throw error;
      });
      return (await fetched).slice(0);
    },
  };
}

/** The last part of the URL's path, decoded; the default name when it has none. */
function nameOf(url: string | URL): string {
  try {
    const path = new URL(String(url), globalThis.location?.href).pathname;
    const last = path.slice(path.lastIndexOf('/') + 1);
    return last ? decodeURIComponent(last) : DEFAULT_NAME;
  } catch {
    return DEFAULT_NAME;
  }
}
