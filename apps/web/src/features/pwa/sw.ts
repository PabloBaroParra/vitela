// The service worker: dist/sw.js, bundled on its own by scripts/pwaPlugin.ts
// after the app build (never imported by the app). The build writes the list
// of files to precache and the build's version in for the two constants.
//
// - install: precache every built file — the app shell, the JS and CSS, the
//   engine worker and both WASM modules (~9 MB) — into this build's cache.
// - activate: drop older builds' caches; take over pages opened before the
//   first install, so the first visit already works offline afterwards.
// - fetch: a precached URL (and every page navigation, as the app shell) is
//   answered from the cache, else from the network, which refills the cache.
//   Nothing else is touched: other origins, non-GETs and anything the build
//   did not write go straight to the network and are never stored. A PDF the
//   user opens never goes through here at all: it is read from the disk.
// - a new deploy is a new worker with a new cache; it waits until the page
//   says SKIP_WAITING (src/features/pwa/updates.ts) or every tab closes.

import { cacheName, precacheKey, staleCaches } from './swRules';
import { SKIP_WAITING } from './updates';

declare const __VITELA_PRECACHE__: readonly string[];
declare const __VITELA_CACHE_VERSION__: string;

/** The service worker events used here (the DOM lib types the page side only). */
interface ExtendableEvent extends Event {
  waitUntil(promise: Promise<unknown>): void;
}
interface FetchEvent extends ExtendableEvent {
  readonly request: Request;
  respondWith(response: Promise<Response>): void;
}
interface MessageEventLike extends ExtendableEvent {
  readonly data: unknown;
}
interface ServiceWorkerScope {
  readonly location: Location;
  readonly clients: { claim(): Promise<void> };
  skipWaiting(): Promise<void>;
  addEventListener(type: 'install' | 'activate', listener: (event: ExtendableEvent) => void): void;
  addEventListener(type: 'fetch', listener: (event: FetchEvent) => void): void;
  addEventListener(type: 'message', listener: (event: MessageEventLike) => void): void;
}

const scope = globalThis as unknown as ServiceWorkerScope;
const CACHE = cacheName(__VITELA_CACHE_VERSION__);
const PRECACHE = new Set(__VITELA_PRECACHE__);

scope.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE).then((cache) =>
      // The shell revalidates past the HTTP cache; the hashed assets may
      // come from it (they are immutable, and usually just downloaded).
      cache.addAll([...PRECACHE].map((url) => new Request(url, { cache: url === '/' ? 'no-cache' : 'default' }))),
    ),
  );
});

scope.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) => Promise.all(staleCaches(names, CACHE).map((name) => caches.delete(name))))
      .then(() => scope.clients.claim()),
  );
});

scope.addEventListener('message', (event) => {
  const data = event.data as { type?: unknown } | null;
  if (data?.type === SKIP_WAITING.type) event.waitUntil(scope.skipWaiting());
});

scope.addEventListener('fetch', (event) => {
  const key = precacheKey(event.request, scope.location.origin, PRECACHE);
  if (key === null) return;
  event.respondWith(fromCache(key, event.request));
});

async function fromCache(key: string, request: Request): Promise<Response> {
  const cache = await caches.open(CACHE);
  const cached = await cache.match(key);
  if (cached) return cached;
  // Not cached (evicted, or the install is still running): the network, and
  // the answer goes back in for next time.
  const response = await fetch(request.mode === 'navigate' ? key : request);
  if (response.ok) await cache.put(key, response.clone());
  return response;
}
