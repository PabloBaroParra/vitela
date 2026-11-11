// Tracks whether the engine worker has finished loading.

import { useEffect, useState } from 'react';
import type { EngineClient } from '../core/client';

export type EngineStatus =
  | { readonly kind: 'loading' }
  | { readonly kind: 'ready' }
  | { readonly kind: 'failed'; readonly message: string };

export function useEngineStatus(client: Pick<EngineClient, 'ready'>): EngineStatus {
  const [status, setStatus] = useState<EngineStatus>({ kind: 'loading' });
  useEffect(() => {
    let live = true;
    client.ready.then(
      () => {
        if (live) setStatus({ kind: 'ready' });
      },
      (error: unknown) => {
        const message = error instanceof Error ? error.message : String(error);
        if (live) setStatus({ kind: 'failed', message });
      },
    );
    return () => {
      live = false;
    };
  }, [client]);
  return status;
}
