// Writing to a file handle: the permission a write needs, and the write
// itself. Both report failure as a value so a caller can fall back.

import type { WritableFileHandle } from './fileHandle';
import { adoptCurrentFile } from './fileStamp';

const READWRITE = { mode: 'readwrite' } as const;

/**
 * Whether the handle may be written, asking the user when it is not granted
 * yet. The prompt needs the user's click still to be fresh, so call this
 * before any slow work, not after it.
 */
export async function ensureWritePermission(handle: WritableFileHandle): Promise<PermissionState> {
  try {
    const current = (await handle.queryPermission?.(READWRITE)) ?? 'granted';
    if (current === 'granted') return current;
    return (await handle.requestPermission?.(READWRITE)) ?? 'denied';
  } catch {
    return 'denied';
  }
}

/** Writes `bytes` as the file's whole content; `false` when the write failed. */
export async function writeToHandle(handle: WritableFileHandle, bytes: Uint8Array): Promise<boolean> {
  let writable: Awaited<ReturnType<WritableFileHandle['createWritable']>> | undefined;
  try {
    // The browser writes to a swap file and replaces the original on close,
    // so a failed write leaves the user's file as it was.
    writable = await handle.createWritable();
    await writable.write(bytes);
    await writable.close();
  } catch {
    // Release the swap file instead of leaving it locked until the page goes.
    try {
      await writable?.abort?.();
    } catch {
      // Nothing more to do: the write already failed.
    }
    return false;
  }
  // Our own write moved the file's modified time: take it as the baseline, so
  // the next save does not mistake it for somebody else's change.
  await adoptCurrentFile(handle);
  return true;
}
