// How a save reaches the user, in two steps. `prepare` is the FIRST step of a
// Save click: anything that needs the user's click still fresh (the write
// permission prompt, the save dialog) is done there, before the engine spends
// time producing the bytes — by then the click's activation may have expired.
// `deliver` then puts the bytes where `prepare` decided.

import { canPickSaveTarget, pickSaveTarget } from '../../platform/saveTarget';
import { ensureWritePermission } from '../../platform/fileWrite';
import type { WritableFileHandle } from '../../platform/fileHandle';
import type { SaveOutcome } from './outcome';
import { saveBytes } from './saveBytes';
import { copyName, saveCopy } from './saveCopy';

/** What the user asked for: write over the opened file, or save a new copy. */
export type SaveIntent = 'overwrite' | 'copy';

/** Where the bytes go, as far as `prepare` could settle it. */
export type Destination =
  | {
      readonly kind: 'overwrite';
      /** The opened file's handle, if the browser gave one. */
      readonly handle: WritableFileHandle | undefined;
      /** The answer to the write-permission request made at the click. */
      readonly permission: PermissionState;
    }
  | {
      readonly kind: 'copy';
      /** The new file the user chose; none where the browser has no save dialog. */
      readonly handle: WritableFileHandle | undefined;
    };

/** The document a save is for. */
export interface SaveSource {
  readonly name: string;
  readonly fileHandle?: WritableFileHandle;
}

export interface DeliverRequest {
  readonly bytes: Uint8Array;
  readonly name: string;
  readonly destination: Destination;
}

export type Deliver = (request: DeliverRequest) => Promise<SaveOutcome>;

export interface Delivery {
  /** `null` when the user dismissed a dialog: the save is cancelled. */
  readonly prepare: (intent: SaveIntent, source: SaveSource) => Promise<Destination | null>;
  readonly deliver: Deliver;
}

async function prepare(intent: SaveIntent, source: SaveSource): Promise<Destination | null> {
  if (intent === 'overwrite') {
    const { fileHandle } = source;
    return { kind: 'overwrite', handle: fileHandle, permission: fileHandle ? await ensureWritePermission(fileHandle) : 'granted' };
  }
  if (!canPickSaveTarget()) return { kind: 'copy', handle: undefined };
  try {
    const handle = await pickSaveTarget(copyName(source.name));
    return handle ? { kind: 'copy', handle } : null;
  } catch {
    // The save dialog itself failed (blocked, say): the copy still arrives, as a download.
    return { kind: 'copy', handle: undefined };
  }
}

const deliver: Deliver = ({ bytes, name, destination }) =>
  destination.kind === 'copy'
    ? saveCopy({ bytes, name, handle: destination.handle })
    : saveBytes({ bytes, name, handle: destination.handle, permission: destination.permission });

/** The delivery the app uses: the browser's file APIs and a download. */
export const browserDelivery: Delivery = { prepare, deliver };
