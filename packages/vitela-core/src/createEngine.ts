// Starts the engine worker and wraps it in the typed client.

import { EngineClient } from './core/client.js';
import type { InitMessage } from './core/protocol.js';

export interface EngineOptions {
  /**
   * A directory URL serving `pdf_wasm_bg.wasm`, `pdfium.js` and `pdfium.wasm`
   * (for example a CDN). A relative URL resolves against the page. Default:
   * the copies shipped in this package, which the bundler emits as assets.
   */
  readonly assetBaseUrl?: string | URL;
  /**
   * The worker script, for a bundler that does not follow
   * `new Worker(new URL('…', import.meta.url))`. It must be a module worker
   * built from this package's `dist/worker/worker.js`. Default: that file.
   */
  readonly workerUrl?: string | URL;
}

/** Spawns the engine's module worker and returns its client. */
export function createEngine(options: EngineOptions = {}): EngineClient {
  // The literal `new Worker(new URL(…))` form is what bundlers look for.
  const worker =
    options.workerUrl === undefined
      ? new Worker(new URL('./worker/worker.js', import.meta.url), { type: 'module', name: 'vitela-engine' })
      : new Worker(options.workerUrl, { type: 'module', name: 'vitela-engine' });
  const client = new EngineClient(worker);
  // A worker whose script fails to load (or throws at top level) never posts
  // a message; without this the UI would wait forever.
  worker.addEventListener('error', (event) => {
    event.preventDefault();
    client.fail(`engine worker failed: ${event.message || 'script did not load'}`);
  });
  worker.postMessage(initMessage(options.assetBaseUrl));
  return client;
}

function initMessage(assetBaseUrl: string | URL | undefined): InitMessage {
  if (assetBaseUrl === undefined) return { type: 'init' };
  // The worker resolves against its own script URL, so make it absolute here.
  return { type: 'init', assetBaseUrl: new URL(String(assetBaseUrl), globalThis.location.href).href };
}
