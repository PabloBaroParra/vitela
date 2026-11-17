import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createEngine } from './createEngine.js';

/** A Worker stand-in recording how it was constructed and what it was sent. */
class FakeWorker extends EventTarget {
  static last: FakeWorker | undefined;
  readonly sent: unknown[] = [];
  constructor(
    readonly url: string | URL,
    readonly options: WorkerOptions,
  ) {
    super();
    FakeWorker.last = this;
  }
  postMessage(message: unknown): void {
    this.sent.push(message);
  }
}

describe('createEngine', () => {
  beforeEach(() => {
    FakeWorker.last = undefined;
    vi.stubGlobal('Worker', FakeWorker);
  });

  it("spawns the package's module worker and sends init without a base URL", () => {
    createEngine();
    const worker = FakeWorker.last!;
    // Under Vitest, Vite rewrites the URL to the worker's source; dist keeps worker.js.
    expect(String(worker.url)).toMatch(/\/worker\/worker\.(js|ts)\b/);
    expect(worker.options).toEqual({ type: 'module', name: 'vitela-engine' });
    expect(worker.sent).toEqual([{ type: 'init' }]);
  });

  it('spawns a custom worker script and resolves both URLs against the page', () => {
    createEngine({ workerUrl: '/static/engine.js', assetBaseUrl: 'wasm/', loadTimeoutMs: 5000 });
    const worker = FakeWorker.last!;
    expect(worker.url).toBe(new URL('/static/engine.js', location.href).href);
    expect(worker.sent).toEqual([
      { type: 'init', assetBaseUrl: new URL('wasm/', location.href).href, loadTimeoutMs: 5000 },
    ]);
  });

  it('throws on an invalid asset base before spawning a worker', () => {
    expect(() => createEngine({ assetBaseUrl: 'http://[' })).toThrow(TypeError);
    expect(() => createEngine({ assetBaseUrl: 'http://[' })).toThrow('invalid assetBaseUrl "http://["');
    expect(FakeWorker.last).toBeUndefined();
  });

  it('throws on an invalid worker URL before spawning a worker', () => {
    expect(() => createEngine({ workerUrl: 'http://[' })).toThrow('invalid workerUrl "http://["');
    expect(FakeWorker.last).toBeUndefined();
  });

  it('throws on a timeout that is not a positive number before spawning a worker', () => {
    expect(() => createEngine({ loadTimeoutMs: -1 })).toThrow(RangeError);
    expect(() => createEngine({ loadTimeoutMs: Number.NaN })).toThrow('invalid loadTimeoutMs NaN');
    expect(FakeWorker.last).toBeUndefined();
  });

  it('fails the client when the worker script does not load', async () => {
    const client = createEngine();
    FakeWorker.last!.dispatchEvent(new ErrorEvent('error', { message: '' }));
    await expect(client.ready).rejects.toThrow('engine worker failed: script did not load');
  });
});
