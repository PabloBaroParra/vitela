// How a save reaches the user, in two steps. `prepare` is the FIRST step of a
// Save click: anything that needs the user's click still fresh (the
// write-permission prompt) is done there, before the engine spends
// time producing the bytes — by then the click's activation may have expired.
// `deliver` then puts the bytes where `prepare` decided.
//
// `prepare` also looks at the opened file: if it changed on disk since the
// document was opened, nothing is written until the user has been asked.

import { adoptCurrentFile, checkFileStamp } from '../../platform/fileStamp';
import { ensureWritePermission } from '../../platform/fileWrite';
import type { WritableFileHandle } from '../../platform/fileHandle';
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
      /** The file could not be checked for outside changes, so it is not written over: download. */
      readonly unverified?: true;
    }
  /** A new copy, always downloaded: see saveCopy.ts for why. */
  | { readonly kind: 'copy' };

/** What `prepare` found: a placement, or that the opened file changed on disk and the user must decide. */
export type Destination = Placement | { readonly kind: 'changedOnDisk' };

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

export type Deliver = (request: DeliverRequest) => Promise<SaveOutcome>;

export interface Delivery {
  /** `null` when the user dismissed a dialog: the save is cancelled. */
  readonly prepare: (intent: SaveIntent, source: SaveSource) => Promise<Destination | null>;
  readonly deliver: Deliver;
}

async function prepare(intent: SaveIntent, source: SaveSource): Promise<Destination | null> {
  if (intent === 'copy') return { kind: 'copy' };
  const { fileHandle } = source;
  if (!fileHandle) return { kind: 'overwrite', handle: undefined, permission: 'granted' };
  // Look at the file first: no point asking for permission to overwrite what the user may not want to.
  if (intent === 'overwriteAnyway') await adoptCurrentFile(fileHandle);
  const check = await checkFileStamp(fileHandle);
  if (check === 'changed') return { kind: 'changedOnDisk' };
  if (check === 'unknown') return { kind: 'overwrite', handle: fileHandle, permission: 'granted', unverified: true };
  return { kind: 'overwrite', handle: fileHandle, permission: await ensureWritePermission(fileHandle) };
}

const deliver: Deliver = ({ bytes, name, destination }) =>
  destination.kind === 'copy'
    ? Promise.resolve(saveCopy({ bytes, name }))
    : saveBytes({
        bytes,
        name,
        handle: destination.handle,
        permission: destination.permission,
        unverified: destination.unverified,
      });

/** The delivery the app uses: the browser's file APIs and a download. */
export const browserDelivery: Delivery = { prepare, deliver };
