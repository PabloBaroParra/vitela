// The service worker's decisions, pure so Vitest can pin them: which cache a
// build uses, which old caches go, and which requests the cache answers.

const PREFIX = 'vitela-';

/** The cache of one build. */
export function cacheName(version: string): string {
  return `${PREFIX}${version}`;
}

/** Older builds' caches, to delete once `current` has taken over. Other caches are left alone. */
export function staleCaches(names: readonly string[], current: string): string[] {
  return names.filter((name) => name.startsWith(PREFIX) && name !== current);
}

/** The parts of a `Request` the worker decides on. */
export interface RequestFacts {
  readonly method: string;
  readonly mode: RequestMode;
  readonly url: string;
}

/**
 * The cache key that answers `request`, or `null` to leave it to the network
 * untouched. Same-origin GETs only: a page navigation gets the app shell (`/`,
 * which serves every tool route), a built asset itself. Anything else —
 * another origin, a POST, a file the build did not write — is never cached.
 */
export function precacheKey(request: RequestFacts, origin: string, precached: ReadonlySet<string>): string | null {
  if (request.method !== 'GET') return null;
  const url = new URL(request.url);
  if (url.origin !== origin) return null;
  if (request.mode === 'navigate') return precached.has('/') ? '/' : null;
  if (url.search !== '') return null;
  return precached.has(url.pathname) ? url.pathname : null;
}
