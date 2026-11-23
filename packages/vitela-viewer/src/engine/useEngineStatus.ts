// Tracks whether the engine worker has finished loading.

import { useEffect, useState } from 'react';
import type { EngineClient } from '@vitela/core';

export type EngineStatus =
  | { readonly kind: 'loading' }
  | { readonly kind: 'ready' }
  | { readonly kind: 'failed'; readonly message: string };

const LOADING: EngineStatus = { kind: 'loading' };

export function useEngineStatus(client: Pick<EngineClient, 'ready'>): EngineStatus {
  // The status belongs to the client that reported it: a new client is loading
  // until it reports, never ready on the strength of the one before it.
  const [reported, setReported] = useState<{ client: Pick<EngineClient, 'ready'>; status: EngineStatus } | null>(null);
  useEffect(() => {
    let live = true;
    client.ready.then(
      () => {
        if (live) setReported({ client, status: { kind: 'ready' } });
      },
      (error: unknown) => {
        const message = error instanceof Error ? error.message : String(error);
        if (live) setReported({ client, status: { kind: 'failed', message } });
      },
    );
    return () => {
      live = false;
    };
  }, [client]);
  return reported?.client === client ? reported.status : LOADING;
}
