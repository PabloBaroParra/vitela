// The delivery of an embedded editor whose host page takes the saved bytes
// (`onSave`): no file to check, no permission to ask, no download. Every save
// intent lands in the same place, the callback, so the destination is only a
// placeholder `deliver` ignores. So does every other file the editor writes
// (a compressed copy, exported page images): the media type tells them apart.

import type { Delivery } from './delivery';
import { PDF_TYPE } from './download';

/**
 * Receives every file the editor writes: its bytes, its name and its media
 * type (`application/pdf`, or `image/png` / `image/jpeg` for an exported
 * page). A rejection fails that write, and the editor says so.
 */
export type SaveCallback = (bytes: Uint8Array, name: string, type: string) => void | Promise<void>;

export function callbackDelivery(onSave: SaveCallback): Delivery {
  return {
    prepare: async () => ({ kind: 'copy' }),
    deliver: async ({ bytes, name, type = PDF_TYPE }) => {
      await onSave(bytes, name, type);
      return { how: 'delivered', name };
    },
  };
}
