// "Save a copy": the bytes go to a NEW file, never over the opened one. That
// is the way out of the signature question when the user wants a copy that
// still verifies next to the original.

import type { WritableFileHandle } from '../../platform/fileHandle';
import { writeToHandle } from '../../platform/fileWrite';
import { downloadBytes } from './download';
import type { SaveOutcome } from './outcome';

/** The name a copy of `name` is suggested under: "report.pdf" -> "report (copy).pdf". */
export function copyName(name: string): string {
  return `${name.replace(/\.pdf$/i, '')} (copy).pdf`;
}

interface SaveCopyRequest {
  readonly bytes: Uint8Array;
  /** The opened document's name; the copy is named after it. */
  readonly name: string;
  /** The new file the user chose; none where the browser has no save dialog. */
  readonly handle: WritableFileHandle | undefined;
  readonly download?: (bytes: Uint8Array, name: string) => void;
}

export async function saveCopy({ bytes, name, handle, download = downloadBytes }: SaveCopyRequest): Promise<SaveOutcome> {
  const suggested = copyName(name);
  if (handle && (await writeToHandle(handle, bytes))) return { how: 'copied', name: handle.name };
  download(bytes, suggested);
  return { how: 'downloaded', name: suggested, fellBack: handle ? 'copy' : null };
}
