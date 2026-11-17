// @vitest-environment node
//
// Server-side rendering: no `location`, and usually no `Worker` either.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createEngine } from './createEngine.js';

describe('createEngine without a page', () => {
  const spawned: unknown[] = [];
  afterEach(() => {
    spawned.length = 0;
  });

  function stubWorker(): void {
    vi.stubGlobal(
      'Worker',
      class {
        constructor(url: unknown) {
          spawned.push(url);
        }
        addEventListener(): void {}
        postMessage(): void {}
      },
    );
  }

  it('throws on a relative asset base, before spawning a worker', () => {
    stubWorker();
    expect(globalThis.location).toBeUndefined();
    expect(() => createEngine({ assetBaseUrl: 'wasm/' })).toThrow(
      'assetBaseUrl "wasm/" must be an absolute URL when there is no page location (server-side rendering)',
    );
    expect(spawned).toEqual([]);
  });

  it('throws on a relative worker URL, before spawning a worker', () => {
    stubWorker();
    expect(() => createEngine({ workerUrl: '/engine.js' })).toThrow(
      'workerUrl "/engine.js" must be an absolute URL when there is no page location',
    );
    expect(spawned).toEqual([]);
  });

  it('accepts absolute URLs without a page', () => {
    stubWorker();
    createEngine({ assetBaseUrl: 'https://cdn.example/v/', workerUrl: 'https://cdn.example/v/worker.js' });
    expect(spawned).toEqual(['https://cdn.example/v/worker.js']);
  });

  it('throws a clear error where there are no Web Workers at all', () => {
    expect(globalThis.Worker).toBeUndefined();
    expect(() => createEngine()).toThrow('createEngine needs Web Workers');
  });
});
