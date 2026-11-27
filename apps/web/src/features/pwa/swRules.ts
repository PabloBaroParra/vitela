// The service worker's decisions, pure so Vitest can pin them: which cache a
// build uses, which old caches go, which requests the cache answers, and
// whether the open tabs let a new build take over.

import { resolveRoute } from '../tools/route';
import { HOLD_QUERY } from './updates';

const PREFIX = 'vitela-';

/** The cache of one build. */
export function cacheName(version: string): string {
  return `${PREFIX}${version}`;
}

/** Older builds' caches, to delete once `current` has taken over. Other caches are left alone. */
export function staleCaches(names: readonly string[], current: string): string[] {
  return names.filter((name) => name.startsWith(PREFIX) && name !== current);
}

/**
 * Whether the app shell answers `pathname`: the home page and each tool
 * route, as the router resolves them (the same routes public/_redirects
 * rewrites). Any other path is the host's, which answers it with 404.html.
 */
export function servesShell(pathname: string): boolean {
  return resolveRoute(pathname).kind !== 'not-found';
}

/** The parts of a `Request` the worker decides on. */
export interface RequestFacts {
  readonly method: string;
  readonly mode: RequestMode;
  readonly url: string;
}

/**
 * The cache key that answers `request`, or `null` to leave it to the network
 * untouched. Same-origin GETs only: a navigation to one of the app's routes
 * gets the app shell (`/`), a built asset itself. Anything else — another
 * origin, a POST, a path the app does not route (the host's 404), a file the
 * build did not write — is never cached.
 */
export function precacheKey(request: RequestFacts, origin: string, precached: ReadonlySet<string>): string | null {
  if (request.method !== 'GET') return null;
  const url = new URL(request.url);
  if (url.origin !== origin) return null;
  if (request.mode === 'navigate') return servesShell(url.pathname) && precached.has('/') ? '/' : null;
  if (url.search !== '') return null;
  return precached.has(url.pathname) ? url.pathname : null;
}

/** A window client, as far as the hold question needs it. */
export interface HoldClient {
  readonly url: string;
  postMessage(message: unknown, transfer: Transferable[]): void;
}

/** The clients that run the app: a 404 page or any other non-app page never answers. */
export function appPages<T extends { readonly url: string }>(clients: readonly T[]): T[] {
  return clients.filter((client) => servesShell(new URL(client.url).pathname));
}

/** How long a tab gets to say whether it holds unsaved edits. */
export const HOLD_TIMEOUT_MS = 2_000;

/**
 * Asks every client whether it holds unsaved edits (src/features/pwa/updates.ts
 * answers on a `MessageChannel`). True if any does — or does not answer in
 * time, or answers anything but a plain "no": a tab that cannot say is
 * treated as holding edits, since taking it over could lose them.
 */
export function anyClientHolds(clients: readonly HoldClient[], timeoutMs = HOLD_TIMEOUT_MS): Promise<boolean> {
  return Promise.all(clients.map((client) => askHold(client, timeoutMs))).then((answers) => answers.some(Boolean));
}

function askHold(client: HoldClient, timeoutMs: number): Promise<boolean> {
  return new Promise((resolve) => {
    const { port1, port2 } = new MessageChannel();
    const done = (held: boolean) => {
      clearTimeout(timer);
      port1.close();
      resolve(held);
    };
    const timer = setTimeout(() => done(true), timeoutMs);
    port1.onmessage = (event: MessageEvent) => done((event.data as { held?: unknown } | null)?.held !== false);
    try {
      client.postMessage(HOLD_QUERY, [port2]);
    } catch {
      done(true);
    }
  });
}
