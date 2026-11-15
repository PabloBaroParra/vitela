// Puts the saved bytes where the user can have them: over the file they
// opened when the browser gave us a handle and its permission, else as a
// download.

import type { WritableFileHandle } from '../../platform/fileHandle';
import { writeToHandle } from '../../platform/fileWrite';
import { downloadBytes } from './download';
import type { SaveOutcome } from './outcome';

interface SaveBytesRequest {
  readonly bytes: Uint8Array;
  /** The document's name, used for a download. */
  readonly name: string;
  readonly handle: WritableFileHandle | undefined;
  /** What the user answered when asked, at the click, to write over `handle`. */
  readonly permission: PermissionState;
  /** The opened file could not be checked for outside changes: never write over it. */
  readonly unverified?: boolean;
  readonly download?: (bytes: Uint8Array, name: string) => void;
}

export async function saveBytes({
  bytes,
  name,
  handle,
  permission,
  unverified = false,
  download = downloadBytes,
}: SaveBytesRequest): Promise<SaveOutcome> {
  if (!handle) {
    download(bytes, name);
    return { how: 'downloaded', name, fellBack: null };
  }
  if (unverified) {
    download(bytes, name);
    return { how: 'downloaded', name, fellBack: 'unverified' };
  }
  if (permission === 'granted') {
    if (await writeToHandle(handle, bytes)) return { how: 'wrote', name: handle.name };
    download(bytes, name);
    return { how: 'downloaded', name, fellBack: 'write' };
  }
  download(bytes, name);
  return { how: 'downloaded', name, fellBack: 'permission' };
}
