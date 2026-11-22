// The delivery of an embedded editor whose host page takes the saved bytes
// (`onSave`): no file to check, no permission to ask, no download. Every save
// intent lands in the same place, the callback, so the destination is only a
// placeholder `deliver` ignores.

import type { Delivery } from './delivery';

/** Receives the saved document; a rejection fails the save, and the editor says so. */
export type SaveCallback = (bytes: Uint8Array, name: string) => void | Promise<void>;

export function callbackDelivery(onSave: SaveCallback): Delivery {
  return {
    prepare: async () => ({ kind: 'copy' }),
    deliver: async ({ bytes, name }) => {
      await onSave(bytes, name);
      return { how: 'delivered', name };
    },
  };
}
