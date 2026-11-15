// Noticing that the file we opened was changed by someone else.
//
// When a document comes in with a file handle, its modified time and size are
// recorded; before a save writes over the file they are read again. A
// difference means the bytes on disk are not the ones the user opened, and
// overwriting them would silently lose that other change.

import type { WritableFileHandle } from './fileHandle';

interface FileStamp {
  readonly lastModified: number;
  readonly size: number;
}

/**
 * `unchanged`: same as when it was opened (or the handle cannot be read, so
 * there is nothing to compare). `changed`: someone else wrote it. `unknown`:
 * the file could not be read, then or now, so nothing can be said.
 */
export type FileCheck = 'unchanged' | 'changed' | 'unknown';

/** One baseline per handle; a promise, so a check made right after opening waits for it. */
const baselines = new WeakMap<WritableFileHandle, Promise<FileStamp | null>>();

async function read(handle: WritableFileHandle): Promise<FileStamp | null> {
  try {
    const file = await handle.getFile?.();
    return file ? { lastModified: file.lastModified, size: file.size } : null;
  } catch {
    return null;
  }
}

/** Takes the file's current state as the one the user opened. Call it when the document is opened. */
export function recordFileStamp(handle: WritableFileHandle): void {
  if (handle.getFile) baselines.set(handle, read(handle));
}

/** Takes the file as it is now as the baseline: after our own write, or once the user accepted the change. */
export async function adoptCurrentFile(handle: WritableFileHandle): Promise<void> {
  if (!handle.getFile) return;
  const now = read(handle);
  baselines.set(handle, now);
  await now;
}

/** Compares the file on disk with the recorded baseline. */
export async function checkFileStamp(handle: WritableFileHandle): Promise<FileCheck> {
  if (!handle.getFile) return 'unchanged';
  const [before, now] = await Promise.all([baselines.get(handle), read(handle)]);
  if (!before || !now) return 'unknown';
  return before.lastModified === now.lastModified && before.size === now.size ? 'unchanged' : 'changed';
}
