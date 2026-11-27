import { describe, expect, it, vi } from 'vitest';
import { TOOLS } from '../tools/tools';
import { anyClientHolds, appPages, cacheName, precacheKey, servesShell, staleCaches, type HoldClient } from './swRules';
import { HOLD_QUERY } from './updates';

const ORIGIN = 'https://vitela.example';
const precached = new Set(['/', '/assets/index-DkBMLp3E.js', '/assets/pdfium-8k7OGqr9.wasm', '/manifest.webmanifest']);

function request(path: string, mode: RequestMode = 'cors', method = 'GET') {
  return { method, mode, url: new URL(path, ORIGIN).href };
}

describe('cacheName / staleCaches', () => {
  it('names one cache per build version', () => {
    expect(cacheName('0123456789abcdef')).toBe('vitela-0123456789abcdef');
  });

  it("drops every older build's cache and leaves caches that are not ours alone", () => {
    expect(staleCaches(['vitela-old1', 'vitela-new2', 'someone-else', 'vitela-old3'], 'vitela-new2')).toEqual([
      'vitela-old1',
      'vitela-old3',
    ]);
  });
});

describe('precacheKey', () => {
  it('answers a built asset from the cache', () => {
    expect(precacheKey(request('/assets/pdfium-8k7OGqr9.wasm'), ORIGIN, precached)).toBe('/assets/pdfium-8k7OGqr9.wasm');
  });

  it("answers a navigation to the app's routes with the app shell", () => {
    expect(precacheKey(request('/', 'navigate'), ORIGIN, precached)).toBe('/');
    expect(precacheKey(request('/organize', 'navigate'), ORIGIN, precached)).toBe('/');
    expect(precacheKey(request('/merge?x=1', 'navigate'), ORIGIN, precached)).toBe('/');
    for (const tool of TOOLS) expect(precacheKey(request(tool.path, 'navigate'), ORIGIN, precached), tool.path).toBe('/');
  });

  it('leaves a navigation to any other path to the host, which answers 404', () => {
    expect(precacheKey(request('/no-such-page', 'navigate'), ORIGIN, precached)).toBeNull();
    expect(precacheKey(request('/organize/extra', 'navigate'), ORIGIN, precached)).toBeNull();
    expect(precacheKey(request('/assets/index-DkBMLp3E.js', 'navigate'), ORIGIN, precached)).toBeNull();
    expect(precacheKey(request('/404.html', 'navigate'), ORIGIN, precached)).toBeNull();
  });

  it('leaves requests to other origins to the network', () => {
    expect(precacheKey(request('https://cdn.example/assets/index-DkBMLp3E.js'), ORIGIN, precached)).toBeNull();
    expect(precacheKey(request('https://cdn.example/', 'navigate'), ORIGIN, precached)).toBeNull();
  });

  it('leaves anything that is not a GET to the network', () => {
    expect(precacheKey(request('/assets/index-DkBMLp3E.js', 'cors', 'POST'), ORIGIN, precached)).toBeNull();
    expect(precacheKey(request('/', 'navigate', 'POST'), ORIGIN, precached)).toBeNull();
  });

  it('never caches what is not in the build, such as a PDF', () => {
    expect(precacheKey(request('/document.pdf'), ORIGIN, precached)).toBeNull();
    expect(precacheKey(request('/assets/unknown.js'), ORIGIN, precached)).toBeNull();
  });

  it('does not answer a built asset asked for with a query string', () => {
    expect(precacheKey(request('/assets/index-DkBMLp3E.js?v=2'), ORIGIN, precached)).toBeNull();
  });
});

describe('servesShell', () => {
  it('is the home page and every tool route, the same paths public/_redirects rewrites', () => {
    expect(servesShell('/')).toBe(true);
    for (const tool of TOOLS) expect(servesShell(tool.path), tool.path).toBe(true);
    expect(servesShell('/nope')).toBe(false);
    expect(servesShell('/sw.js')).toBe(false);
  });
});

/** A tab that answers the hold question with `held`, never (`'silent'`), or with nonsense (`'garbled'`). */
function tab(path: string, held: boolean | 'silent' | 'garbled'): HoldClient & { asked: unknown[] } {
  const asked: unknown[] = [];
  return {
    url: new URL(path, ORIGIN).href,
    asked,
    postMessage(message, transfer) {
      asked.push(message);
      const port = transfer[0] as MessagePort;
      if (held === 'silent') return;
      port.postMessage(held === 'garbled' ? 'yes?' : { held });
    },
  };
}

describe('appPages', () => {
  it('keeps only the clients that run the app, not a 404 page', () => {
    const clients = [tab('/', false), tab('/edit', false), tab('/missing', false)];
    expect(appPages(clients).map((client) => new URL(client.url).pathname)).toEqual(['/', '/edit']);
  });
});

describe('anyClientHolds', () => {
  it('is false when every tab says it has no unsaved edits', async () => {
    const clients = [tab('/', false), tab('/edit', false)];
    await expect(anyClientHolds(clients)).resolves.toBe(false);
    for (const client of clients) expect(client.asked).toEqual([HOLD_QUERY]);
  });

  it('is true when any tab holds unsaved edits', async () => {
    await expect(anyClientHolds([tab('/', false), tab('/edit', true)])).resolves.toBe(true);
  });

  it('is false with no tab open', async () => {
    await expect(anyClientHolds([])).resolves.toBe(false);
  });

  it('counts a tab that does not answer in time as holding edits', async () => {
    vi.useFakeTimers();
    try {
      const answer = anyClientHolds([tab('/', false), tab('/edit', 'silent')], 50);
      await vi.advanceTimersByTimeAsync(60);
      await expect(answer).resolves.toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it('counts an answer that is not a plain no as holding edits', async () => {
    await expect(anyClientHolds([tab('/', 'garbled')])).resolves.toBe(true);
  });

  it('counts a tab that cannot be asked as holding edits', async () => {
    const broken: HoldClient = {
      url: new URL('/', ORIGIN).href,
      postMessage: () => {
        throw new Error('gone');
      },
    };
    await expect(anyClientHolds([broken])).resolves.toBe(true);
  });
});
