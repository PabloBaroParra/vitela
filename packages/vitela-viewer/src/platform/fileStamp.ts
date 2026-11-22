// Noticing that the file we opened was changed by someone else.
//
// When a document comes in with a file handle, its modified time and size are
// recorded; before a save writes over the file they are read again. A
// difference means the bytes on disk are not the ones the user opened, and
// overwriting them would silently lose that other change.

import type { WritableFileHandle } from './fileHandle';

export interface FileStamp {
  readonly lastModified: number;
  readonly size: number;
}

/**
 * `unchanged`: same as when it was opened (or the handle cannot be read, so
 * there is nothing to compare). `changed`: someone else wrote it. `unknown`:
 * the file cannot be read right now, so nothing can be said (a later check
 * tries again).
 */
export type FileCheck = 'unchanged' | 'changed' | 'unknown';

/** One baseline per handle; a promise, so a check made right after opening waits for it. */
const baselines = new WeakMap<WritableFileHandle, Promise<FileStamp | null>>();

/** The file's current stamp; `null` when it cannot be read. */
export async function readFileStamp(handle: WritableFileHandle): Promise<FileStamp | null> {
  try {
    const file = await handle.getFile?.();
    return file ? { lastModified: file.lastModified, size: file.size } : null;
  } catch {
    return null;
  }
}

export function sameStamp(a: FileStamp, b: FileStamp): boolean {
  return a.lastModified === b.lastModified && a.size === b.size;
}

/** Takes the file's current state as the one the user opened. Call it when the document is opened. */
export function recordFileStamp(handle: WritableFileHandle): void {
  if (handle.getFile) baselines.set(handle, readFileStamp(handle));
}

/** Takes the file as it is now as the baseline: only after our own write succeeded. */
export async function adoptCurrentFile(handle: WritableFileHandle): Promise<void> {
  if (!handle.getFile) return;
  const now = readFileStamp(handle);
  baselines.set(handle, now);
  await now;
}

/**
 * Compares the file on disk with the recorded baseline. A baseline that could
 * not be read at open stays "unknown": adopting a later read would hide a
 * change made in between, so only our own successful write sets a new one.
 */
export async function checkFileStamp(handle: WritableFileHandle): Promise<FileCheck> {
  if (!handle.getFile) return 'unchanged';
  const recorded = baselines.get(handle);
  if (!recorded) return 'unknown';
  const [before, now] = await Promise.all([recorded, readFileStamp(handle)]);
  if (!now || !before) return 'unknown';
  return sameStamp(before, now) ? 'unchanged' : 'changed';
}
