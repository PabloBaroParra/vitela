import { describe, expect, it } from 'vitest';
import { EngineClient } from './client';
import { EngineError } from './errors';
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
    expect(message.request).toEqual({ kind: 'open', bytes, password: undefined });
    expect(transfer).toEqual([bytes.buffer]);

    const pages = [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }];
    const value = { handle: 3, info: { pageCount: 1, pages, textExtractionAllowed: true } };
    reply({ type: 'result', id: message.id, value });
    await expect(opened).resolves.toEqual(value);
  });

  it('correlates out-of-order replies by request id', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const first = client.renderPage(0, 0, 100);
    const second = client.renderPage(0, 1, 200);
    const [a, b] = sent.map((s) => s.message.id) as [number, number];
    expect(a).not.toBe(b);

    const page = { width: 2, height: 1, rgba: new Uint8ClampedArray(8) };
    reply({ type: 'error', id: b, name: 'PageIndexOutOfBounds', message: 'page 1: out of range' });
    reply({ type: 'result', id: a, value: page });

    await expect(first).resolves.toBe(page);
    await expect(second).rejects.toThrow('page 1: out of range');
  });

  it('fails pending and future requests when the worker dies', async () => {
    const { port, sent } = fakePort();
    const client = new EngineClient(port);
    const pending = client.renderPage(0, 0, 100);
    client.fail('worker script did not load');
    await expect(pending).rejects.toThrow('worker script did not load');
    await expect(client.ready).rejects.toThrow('worker script did not load');
    await expect(client.renderPage(0, 0, 100)).rejects.toThrow('worker script did not load');
    expect(sent).toHaveLength(1);
  });

  it('ignores replies for ids it never sent', () => {
    const { port, reply } = fakePort();
    new EngineClient(port);
    expect(() => reply({ type: 'result', id: 999, value: undefined })).not.toThrow();
  });

  it('sends the password with an open request', () => {
    const { port, sent } = fakePort();
    const client = new EngineClient(port);
    const bytes = new Uint8Array([1]);
    void client.open(bytes, 'user-aes-pass');
    expect(sent[0]!.message.request).toEqual({ kind: 'open', bytes, password: 'user-aes-pass' });
  });

  it('names the document in every render and close request', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    void client.renderPage(4, 2, 96);
    const closed = client.close(4);
    expect(sent.map((s) => s.message.request)).toEqual([
      { kind: 'renderPage', document: 4, pageIndex: 2, dpi: 96 },
      { kind: 'close', document: 4 },
    ]);
    reply({ type: 'result', id: sent[1]!.message.id, value: undefined });
    await expect(closed).resolves.toBeUndefined();
  });

  it('names the document and page in every text request', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const found = client.search(4, 'beta');
    void client.selectText(4, 1, { x: 1, y: 2 }, { x: 3, y: 4 });
    void client.selectWord(4, 2, { x: 5, y: 6 });
    expect(sent.map((s) => s.message.request)).toEqual([
      { kind: 'search', document: 4, query: 'beta' },
      { kind: 'selectText', document: 4, pageIndex: 1, anchor: { x: 1, y: 2 }, focus: { x: 3, y: 4 } },
      { kind: 'selectWord', document: 4, pageIndex: 2, point: { x: 5, y: 6 } },
    ]);
    const hits = [{ pageIndex: 0, rects: new Float32Array([1, 2, 3, 4]) }];
    reply({ type: 'result', id: sent[0]!.message.id, value: hits });
    await expect(found).resolves.toBe(hits);
  });

  it('rejects a text read of a restricted document as TextExtractionDenied', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const found = client.search(4, 'beta');
    reply({ type: 'error', id: sent[0]!.message.id, name: 'TextExtractionDenied', message: 'text extraction is not permitted' });
    await expect(found).rejects.toMatchObject({ name: 'TextExtractionDenied' });
  });

  it('rejects with an EngineError that carries the forwarded name', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const opened = client.open(new Uint8Array([1]));
    reply({ type: 'error', id: sent[0]!.message.id, name: 'PasswordRequired', message: 'needs a password' });

    const error = await opened.catch((reason: unknown) => reason);
    expect(error).toBeInstanceOf(EngineError);
    expect(error).toMatchObject({ name: 'PasswordRequired', message: 'needs a password' });
  });
});
