// Starts the engine worker and wraps it in the typed client.

import { EngineClient } from './core/client.js';
import type { InitMessage } from './core/protocol.js';

export interface EngineOptions {
  /**
   * A directory URL serving `pdf_wasm_bg.wasm`, `pdfium.js` and `pdfium.wasm`
   * (for example a CDN). A relative URL resolves against the page. The files
   * must be the ones this package version ships: the worker checks their
   * sha256 and fails the engine on a mismatch. Default: the copies shipped
   * in this package, which the bundler emits as assets (not checked).
   */
  readonly assetBaseUrl?: string | URL;
  /**
   * The worker script, for a bundler that does not follow
   * `new Worker(new URL('…', import.meta.url))`. It must be a module worker
   * built from this package's `dist/worker/worker.js`. A relative URL
   * resolves against the page. Default: that file.
   */
  readonly workerUrl?: string | URL;
  /**
   * How long each asset fetch may take, in milliseconds, before the engine
   * fails with a timeout naming the file. Default: 60000.
   */
  readonly loadTimeoutMs?: number;
}

/**
 * Spawns the engine's module worker and returns its client. Throws, before
 * any worker exists, when an option is invalid or a relative URL has no page
 * to resolve against (server-side rendering).
 */
export function createEngine(options: EngineOptions = {}): EngineClient {
  // Everything that can fail synchronously runs before the worker is spawned.
  const init = initMessage(options);
  const workerUrl = options.workerUrl === undefined ? undefined : absoluteUrl(options.workerUrl, 'workerUrl');
  if (typeof Worker === 'undefined') {
    throw new Error('createEngine needs Web Workers; call it in the browser, not during server-side rendering');
  }
  // The literal `new Worker(new URL(…))` form is what bundlers look for.
  const worker =
    workerUrl === undefined
      ? new Worker(new URL('./worker/worker.js', import.meta.url), { type: 'module', name: 'vitela-engine' })
      : new Worker(workerUrl, { type: 'module', name: 'vitela-engine' });
  const client = new EngineClient(worker);
  // A worker whose script fails to load (or throws at top level) never posts
  // a message; without this the UI would wait forever.
  worker.addEventListener('error', (event) => {
    event.preventDefault();
    client.fail(`engine worker failed: ${event.message || 'script did not load'}`);
  });
  worker.postMessage(init);
  return client;
}

function initMessage({ assetBaseUrl, loadTimeoutMs }: EngineOptions): InitMessage {
  if (loadTimeoutMs !== undefined && !(Number.isFinite(loadTimeoutMs) && loadTimeoutMs > 0)) {
    throw new RangeError(`invalid loadTimeoutMs ${String(loadTimeoutMs)}: must be a positive number`);
  }
  return {
    type: 'init',
    // The worker resolves against its own script URL, so make it absolute here.
    ...(assetBaseUrl === undefined ? {} : { assetBaseUrl: absoluteUrl(assetBaseUrl, 'assetBaseUrl') }),
    ...(loadTimeoutMs === undefined ? {} : { loadTimeoutMs }),
  };
}

/** `url` resolved against the page; throws a TypeError naming the option. */
function absoluteUrl(url: string | URL, option: string): string {
  const page = globalThis.location?.href;
  try {
    return new URL(String(url), page).href;
  } catch {
    throw new TypeError(
      page === undefined
        ? `${option} "${String(url)}" must be an absolute URL when there is no page location (server-side rendering)`
        : `invalid ${option} "${String(url)}"`,
    );
  }
}
