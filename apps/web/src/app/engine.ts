// Starts the engine worker and wraps it in the typed client.

import { EngineClient } from '../core/client';

export function startEngine(): EngineClient {
  const worker = new Worker(new URL('../worker/worker.ts', import.meta.url), {
    type: 'module',
    name: 'vitela-engine',
  });
  const client = new EngineClient(worker);
  // A worker whose script fails to load (or throws at top level) never posts
  // a message; without this the UI would wait forever.
  worker.addEventListener('error', (event) => {
    event.preventDefault();
    client.fail(`engine worker failed: ${event.message || 'script did not load'}`);
  });
  return client;
}
