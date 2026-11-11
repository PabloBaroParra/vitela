import { describe, expect, it } from 'vitest';
import { EngineClient } from './client';
import type { MessagePortLike, RequestEnvelope, WorkerMessage } from './protocol';

/** A port whose sent messages are recorded and whose replies the test drives. */
function fakePort() {
  const sent: { message: RequestEnvelope; transfer?: Transferable[] }[] = [];
  const listeners: ((event: MessageEvent<WorkerMessage>) => void)[] = [];
  const port: MessagePortLike<WorkerMessage, RequestEnvelope> = {
    postMessage: (message, transfer) => sent.push({ message, transfer }),
    addEventListener: (_type, listener) => listeners.push(listener),
  };
  const reply = (data: WorkerMessage) =>
    listeners.forEach((listener) => listener(new MessageEvent('message', { data })));
  return { port, sent, reply };
}

describe('EngineClient', () => {
  it('resolves ready when the worker reports ready', async () => {
    const { port, reply } = fakePort();
    const client = new EngineClient(port);
    reply({ type: 'ready' });
    await expect(client.ready).resolves.toBeUndefined();
  });

  it('rejects ready and every later call when the worker reports a fatal error', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    reply({ type: 'fatal', message: 'pdfium runtime aborted' });
    await expect(client.ready).rejects.toThrow('pdfium runtime aborted');
    await expect(client.open(new Uint8Array([1]))).rejects.toThrow('pdfium runtime aborted');
    expect(sent).toHaveLength(0);
  });

  it('transfers the document bytes and resolves open with the matching reply', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const bytes = new Uint8Array([37, 80, 68, 70]);
    const opened = client.open(bytes);

    expect(sent).toHaveLength(1);
    const [{ message, transfer }] = sent as [(typeof sent)[number]];
    expect(message.request).toEqual({ kind: 'open', bytes });
    expect(transfer).toEqual([bytes.buffer]);

    reply({ type: 'result', id: message.id, value: { pageCount: 3 } });
    await expect(opened).resolves.toEqual({ pageCount: 3 });
  });

  it('correlates out-of-order replies by request id', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const first = client.renderPage(0, 100);
    const second = client.renderPage(1, 200);
    const [a, b] = sent.map((s) => s.message.id) as [number, number];
    expect(a).not.toBe(b);

    const page = { width: 2, height: 1, rgba: new Uint8ClampedArray(8) };
    reply({ type: 'error', id: b, message: 'page 1: out of range' });
    reply({ type: 'result', id: a, value: page });

    await expect(first).resolves.toBe(page);
    await expect(second).rejects.toThrow('page 1: out of range');
  });

  it('fails pending and future requests when the worker dies', async () => {
    const { port, sent } = fakePort();
    const client = new EngineClient(port);
    const pending = client.renderPage(0, 100);
    client.fail('worker script did not load');
    await expect(pending).rejects.toThrow('worker script did not load');
    await expect(client.ready).rejects.toThrow('worker script did not load');
    await expect(client.renderPage(0, 100)).rejects.toThrow('worker script did not load');
    expect(sent).toHaveLength(1);
  });

  it('ignores replies for ids it never sent', () => {
    const { port, reply } = fakePort();
    new EngineClient(port);
    expect(() => reply({ type: 'result', id: 999, value: { pageCount: 1 } })).not.toThrow();
  });
});
