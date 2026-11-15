// What a save did, for the status line.

/** Why a save that had a file to write over was downloaded instead. */
export type FallbackReason = 'permission' | 'write';

export type SaveOutcome =
  /** Written over the file the document was opened from. */
  | { readonly how: 'wrote'; readonly name: string }
  /** Downloaded as a new copy; the opened file is untouched. */
  | { readonly how: 'copied'; readonly name: string }
  /** Handed to the browser as a download; `fellBack` says why a file handle was not used. */
  | { readonly how: 'downloaded'; readonly name: string; readonly fellBack: FallbackReason | null }
  /** The user dismissed the save dialog: nothing was written. */
  | { readonly how: 'cancelled' };
