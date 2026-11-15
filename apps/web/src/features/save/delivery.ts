// How a save reaches the user, in two steps. `prepare` is the FIRST step of a
// Save click: anything that needs the user's click still fresh (the
// write-permission prompt) is done there, before the engine spends
// time producing the bytes — by then the click's activation may have expired.
// `deliver` then puts the bytes where `prepare` decided.
//
// `prepare` also looks at the opened file: if it changed on disk since the
// document was opened, nothing is written until the user has been asked.

import { checkFileStamp, readFileStamp, sameStamp, type FileStamp } from '../../platform/fileStamp';
import { ensureWritePermission } from '../../platform/fileWrite';
import type { WritableFileHandle } from '../../platform/fileHandle';
import { downloadBytes } from './download';
import type { SaveOutcome } from './outcome';
import { saveBytes } from './saveBytes';
import { saveCopy } from './saveCopy';

/**
 * What the user asked for: write over the opened file, save a new copy, or
 * write over the file after being told it changed on disk ("overwriteAnyway").
 */
export type SaveIntent = 'overwrite' | 'overwriteAnyway' | 'copy';

/** Where the bytes go, once `prepare` has settled it. */
export type Placement =
  | {
      readonly kind: 'overwrite';
      /** The opened file's handle, if the browser gave one. */
      readonly handle: WritableFileHandle | undefined;
      /** The answer to the write-permission request made at the click. */
      readonly permission: PermissionState;
      /** The file as `prepare` saw it: if it moved before the write, nothing is written. */
      readonly seen?: FileStamp;
    }
  /** The opened file could not be checked for outside changes, so it is not written over: download. */
  | { readonly kind: 'unverified' }
  /** A new copy, always downloaded: see saveCopy.ts for why. */
  | { readonly kind: 'copy' };

/** What `prepare` found: a placement, or that the opened file changed on disk and the user must decide. */
export type Destination = Placement | { readonly kind: 'changedOnDisk' };

/** What `deliver` did: an outcome, or that the file changed while the bytes were being built. */
export type Delivered = SaveOutcome | { readonly how: 'changedOnDisk' };

/** The document a save is for. */
export interface SaveSource {
  readonly name: string;
  readonly fileHandle?: WritableFileHandle;
}

export interface DeliverRequest {
  readonly bytes: Uint8Array;
  readonly name: string;
  readonly destination: Placement;
}

export type Deliver = (request: DeliverRequest) => Promise<Delivered>;

export interface Delivery {
  /** `null` when the user dismissed a dialog: the save is cancelled. */
  readonly prepare: (
    intent: SaveIntent,
    source: SaveSource,
    /** The stamp of a change the user already accepted in this save: compared against instead of the baseline. */
    accepted?: FileStamp,
  ) => Promise<Destination | null>;
  readonly deliver: Deliver;
}

async function prepare(intent: SaveIntent, source: SaveSource, accepted?: FileStamp): Promise<Destination | null> {
  if (intent === 'copy') return { kind: 'copy' };
  const { fileHandle } = source;
  if (!fileHandle) return { kind: 'overwrite', handle: undefined, permission: 'granted' };
  // The permission prompt needs the click's activation, so it comes before any file read.
  const permission = await ensureWritePermission(fileHandle);
  // Refused: the save is a download, and a download overwrites nothing.
  if (permission !== 'granted') return { kind: 'overwrite', handle: fileHandle, permission };
  if (intent === 'overwriteAnyway') {
    // The user accepted the change, but it is only adopted once the write succeeded.
    const seen = await readFileStamp(fileHandle);
    return seen || !fileHandle.getFile
      ? { kind: 'overwrite', handle: fileHandle, permission, seen: seen ?? undefined }
      : { kind: 'unverified' };
  }
  if (accepted) {
    // A retry of a save whose change the user accepted: only a NEWER change asks again.
    const now = await readFileStamp(fileHandle);
    if (!now) return { kind: 'unverified' };
    return sameStamp(accepted, now)
      ? { kind: 'overwrite', handle: fileHandle, permission, seen: accepted }
      : { kind: 'changedOnDisk' };
  }
  const check = await checkFileStamp(fileHandle);
  if (check === 'changed') return { kind: 'changedOnDisk' };
  if (check === 'unknown') return { kind: 'unverified' };
  const seen = await readFileStamp(fileHandle);
  // No stamp to guard the write with: not verified, so no write over the file.
  return seen || !fileHandle.getFile
    ? { kind: 'overwrite', handle: fileHandle, permission, seen: seen ?? undefined }
    : { kind: 'unverified' };
}

async function deliver({ bytes, name, destination }: DeliverRequest): Promise<Delivered> {
  switch (destination.kind) {
    case 'copy':
      return saveCopy({ bytes, name });
    case 'unverified':
      downloadBytes(bytes, name);
      return { how: 'downloaded', name, fellBack: 'unverified' };
    case 'overwrite': {
      const { handle, permission, seen } = destination;
      // The engine may have taken a while: look again right before writing.
      if (handle && permission === 'granted' && seen) {
        const now = await readFileStamp(handle);
        if (!now || !sameStamp(seen, now)) return { how: 'changedOnDisk' };
      }
      return saveBytes({ bytes, name, handle, permission });
    }
  }
}

/** The delivery the app uses: the browser's file APIs and a download. */
export const browserDelivery: Delivery = { prepare, deliver };
