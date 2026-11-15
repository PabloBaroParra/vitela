// Puts the saved bytes where the user can have them: over the file they
// opened when the browser gave us a handle, else as a download.

import { downloadBytes } from './download';
import type { WritableFileHandle } from '../../platform/fileHandle';

export type SaveOutcome =
  /** Written over the file the document was opened from. */
  | { readonly how: 'wrote'; readonly name: string }
  /** Handed to the browser as a download; `fellBack` when a handle existed but could not be used. */
  | { readonly how: 'downloaded'; readonly name: string; readonly fellBack: boolean };

interface SaveBytesRequest {
  readonly bytes: Uint8Array;
  /** The document's name, used for a download. */
  readonly name: string;
  readonly handle: WritableFileHandle | undefined;
  readonly download?: (bytes: Uint8Array, name: string) => void;
}

export async function saveBytes({ bytes, name, handle, download = downloadBytes }: SaveBytesRequest): Promise<SaveOutcome> {
  if (handle && (await writeOver(handle, bytes))) return { how: 'wrote', name: handle.name };
  download(bytes, name);
  return { how: 'downloaded', name, fellBack: handle !== undefined };
}

/** Writes `bytes` over the file; `false` when the user refused or the write failed. */
async function writeOver(handle: WritableFileHandle, bytes: Uint8Array): Promise<boolean> {
  try {
    const mode = { mode: 'readwrite' } as const;
    let permission = (await handle.queryPermission?.(mode)) ?? 'granted';
    if (permission !== 'granted') permission = (await handle.requestPermission?.(mode)) ?? 'denied';
    if (permission !== 'granted') return false;
    // The browser writes to a swap file and replaces the original on close,
    // so a failed write leaves the user's file as it was.
    const writable = await handle.createWritable();
    await writable.write(bytes);
    await writable.close();
    return true;
  } catch {
    return false;
  }
}
