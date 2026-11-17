// Web Worker entry: waits for the client's init message (where the WASM
// lives), loads the engine, reports ready (or fatal), then serves requests
// one at a time — the worker is the engine's actor.

import type { ClientMessage, InitMessage, MessagePortLike, WorkerMessage } from '../core/protocol.js';
import { loadEngine } from './engine.js';
import { errorMessage } from '../core/errors.js';
import { handleRequest } from './handler.js';
import { resolveAssetUrls } from './assets.js';

const port = self as unknown as MessagePortLike<ClientMessage, WorkerMessage>;

// Requests that arrive before init wait on this, like any request waits for
// the engine to load.
let init!: (message: InitMessage) => void;
const engine = new Promise<InitMessage>((resolve) => {
  init = resolve;
}).then(({ assetBaseUrl }) => loadEngine(resolveAssetUrls(assetBaseUrl)));

engine.then(
  () => port.postMessage({ type: 'ready' }),
  (error: unknown) => port.postMessage({ type: 'fatal', message: errorMessage(error) }),
);

// Requests are chained so pdfium never sees two at once, even if a future
// engine method awaits mid-request.
let queue: Promise<unknown> = Promise.resolve();
port.addEventListener('message', ({ data }) => {
  if ('type' in data) {
    init(data);
    return;
  }
  queue = queue.then(async () => {
    const reply = await handleRequest(await engine, data);
    port.postMessage(reply.message, reply.transfer);
  });
  // A dead engine was already reported as fatal; do not reject the chain.
  queue = queue.catch(() => {});
});
