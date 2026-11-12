// Web Worker entry: loads the engine, reports ready (or fatal), then serves
// requests one at a time — the worker is the engine's actor.

import type { MessagePortLike, RequestEnvelope, WorkerMessage } from '../core/protocol';
import { createEngine } from './engine';
import { errorMessage } from '../core/errors';
import { handleRequest } from './handler';

const port = self as unknown as MessagePortLike<RequestEnvelope, WorkerMessage>;
const engine = createEngine();

engine.then(
  () => port.postMessage({ type: 'ready' }),
  (error: unknown) => port.postMessage({ type: 'fatal', message: errorMessage(error) }),
);

// Requests are chained so pdfium never sees two at once, even if a future
// engine method awaits mid-request.
let queue: Promise<unknown> = Promise.resolve();
port.addEventListener('message', ({ data }) => {
  queue = queue.then(async () => {
    const reply = await handleRequest(await engine, data);
    port.postMessage(reply.message, reply.transfer);
  });
  // A dead engine was already reported as fatal; do not reject the chain.
  queue = queue.catch(() => {});
});
