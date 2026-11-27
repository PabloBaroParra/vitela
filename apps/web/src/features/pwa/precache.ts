// What the service worker precaches, and the version its cache is named
// after. Pure: the build (scripts/pwaPlugin.ts) hands in the files it wrote
// and a digest function, so this runs the same in Node and in Vitest.
//
// Only files the build wrote are ever precached. A user's PDF is never one of
// them (it is opened from the disk and never fetched over HTTP), and `.pdf`
// is refused here even so.

/** One file the build wrote: its path inside dist/ and a digest of its bytes. */
export interface BuiltFile {
  readonly path: string;
  readonly digest: string;
}

/** dist/ files the worker must not cache: itself, host config, the not-found page. */
const LEFT_OUT = new Set(['sw.js', '_headers', '_redirects', '404.html']);

function toUrlPath(path: string): string {
  return path.replace(/\\/g, '/').replace(/^\/+/, '');
}

function precached(path: string): boolean {
  return !LEFT_OUT.has(path) && !path.endsWith('.map') && !/\.pdf$/i.test(path);
}

/** The URLs to precache, sorted: `index.html` is the app shell, `/`. */
export function precacheUrls(paths: readonly string[]): string[] {
  const urls = paths
    .map(toUrlPath)
    .filter(precached)
    .map((path) => (path === 'index.html' ? '/' : `/${path}`));
  return [...new Set(urls)].sort();
}

/**
 * A 16-hex-digit version of the precached files, names and bytes both: any
 * change to what a deploy serves gives a new version, so a new cache, and the
 * old one is dropped when the new worker takes over. `hash` must return hex.
 */
export function cacheVersion(files: readonly BuiltFile[], hash: (text: string) => string): string {
  const lines = files
    .map((file) => ({ path: toUrlPath(file.path), digest: file.digest }))
    .filter((file) => precached(file.path))
    .map((file) => `${file.path}\0${file.digest}`)
    .sort();
  return hash(lines.join('\n')).slice(0, 16);
}
