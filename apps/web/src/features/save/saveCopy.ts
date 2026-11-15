// "Save a copy": the bytes go to a NEW file, never over the opened one. That
// is the way out of the signature question when the user wants a copy that
// still verifies next to the original.
//
// It is always a download. Chromium's showSaveFilePicker truncates an existing
// file it hands back (file_system_access_manager_impl.cc,
// DidCreateAndTruncateSaveFile), so picking the opened file there would empty
// it before anything could refuse; a download never overwrites a file.

import { downloadBytes } from './download';
import type { SaveOutcome } from './outcome';

/** The name a copy of `name` is saved under: "report.pdf" -> "report (copy).pdf". */
export function copyName(name: string): string {
  return `${name.replace(/.pdf$/i, '')} (copy).pdf`;
}

interface SaveCopyRequest {
  readonly bytes: Uint8Array;
  /** The opened document's name; the copy is named after it. */
  readonly name: string;
  readonly download?: (bytes: Uint8Array, name: string) => void;
}

export function saveCopy({ bytes, name, download = downloadBytes }: SaveCopyRequest): SaveOutcome {
  const copy = copyName(name);
  download(bytes, copy);
  return { how: 'copied', name: copy };
}
