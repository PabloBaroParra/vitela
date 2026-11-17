import { describe, expect, it } from 'vitest';
import type { ClientMessage, MessagePortLike, WorkerMessage } from '../core/protocol.js';
import type { Engine } from '../core/types.js';
import type { AssetPlan } from './assets.js';
import { serveEngine } from './serve.js';

/** The worker's side of the channel: the test sends, the worker posts. */
class FakePort implements MessagePortLike<ClientMessage, WorkerMessage> {
  readonly posted: WorkerMessage[] = [];
  private listener?: (event: MessageEvent<ClientMessage>) => void;

  postMessage(message: WorkerMessage): void {
    this.posted.push(message);
  }
  addEventListener(_type: 'message', listener: (event: MessageEvent<ClientMessage>) => void): void {
    this.listener = listener;
  }
  send(data: ClientMessage): void {
    this.listener?.({ data } as MessageEvent<ClientMessage>);
  }
}

const engine = { close: async () => {} } as unknown as Engine;
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('serveEngine', () => {
  it('holds a request sent before init and answers it once the engine has loaded', async () => {
    const port = new FakePort();
    const plans: AssetPlan[] = [];
    serveEngine(
      port,
      async (plan) => {
        plans.push(plan);
        return engine;
      },
      {},
    );

    port.send({ id: 1, request: { kind: 'close', document: 3 } });
    await settle();
    expect(plans).toEqual([]);
    expect(port.posted).toEqual([]);

    port.send({ type: 'init', loadTimeoutMs: 1234 });
    await settle();
    expect(plans).toHaveLength(1);
    expect(plans[0]?.timeoutMs).toBe(1234);
    expect(port.posted).toEqual([{ type: 'ready' }, { type: 'result', id: 1, value: undefined }]);
  });

  it('reports fatal, without loading, when init carries a bad asset base', async () => {
    const port = new FakePort();
    let loads = 0;
    serveEngine(
      port,
      async () => {
        loads++;
        return engine;
      },
      {},
    );

    port.send({ type: 'init', assetBaseUrl: 'not a url' });
    await settle();
    expect(loads).toBe(0);
    expect(port.posted).toEqual([
      { type: 'fatal', message: 'invalid assetBaseUrl "not a url": must be an absolute URL' },
    ]);
  });

  it('passes the package digests along for a custom base', async () => {
    const port = new FakePort();
    const digests = { 'pdfium.js': 'aa' };
    let plan: AssetPlan | undefined;
    serveEngine(
      port,
      async (p) => {
        plan = p;
        return engine;
      },
      digests,
    );
    port.send({ type: 'init', assetBaseUrl: 'https://cdn.example/v/' });
    await settle();
    expect(plan?.integrity).toBe(digests);
  });
});
