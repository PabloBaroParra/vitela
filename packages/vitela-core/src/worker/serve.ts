// The worker's message loop, apart from `self` so it can be tested: waits for
// the client's init message, loads the engine, reports ready (or fatal),
// then serves requests one at a time — the worker is the engine's actor.

import { errorMessage } from '../core/errors.js';
import type { ClientMessage, InitMessage, MessagePortLike, WorkerMessage } from '../core/protocol.js';
import type { Engine } from '../core/types.js';
import { planAssets, type AssetDigests, type AssetPlan } from './assets.js';
import { handleRequest } from './handler.js';

export type LoadEngine = (plan: AssetPlan) => Promise<Engine>;

export function serveEngine(
  port: MessagePortLike<ClientMessage, WorkerMessage>,
  load: LoadEngine,
  digests: AssetDigests,
): void {
  // Requests that arrive before init wait on this, like any request waits for
  // the engine to load.
  let init!: (message: InitMessage) => void;
  const engine = new Promise<InitMessage>((resolve) => {
    init = resolve;
  }).then((message) => load(planAssets(message, digests)));

  engine.then(
    () => port.postMessage({ type: 'ready' }),
    (error: unknown) => port.postMessage({ type: 'fatal', message: errorMessage(error) }),
  );

  // Requests are chained so pdfium never sees two at once, even if a future
  // engine method awaits mid-request.
  let queue: Promise<unknown> = Promise.resolve();
  port.addEventListener('message', ({ data }) => {
    if ('type' in data) {
      // Only the first init counts; the engine loads once.
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
}
