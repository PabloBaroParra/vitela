import { describe, expect, it } from 'vitest';
import { cacheVersion, precacheUrls } from './precache';

/** A stand-in for the build's SHA-256: 64 hex digits, eight seeded FNV-1a rounds. */
function hash(text: string): string {
  let hex = '';
  for (let seed = 0; seed < 8; seed++) {
    let h = (0x811c9dc5 ^ seed) >>> 0;
    for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193) >>> 0;
    hex += h.toString(16).padStart(8, '0');
  }
  return hex;
}

describe('precacheUrls', () => {
  it('takes the app shell as "/" and every built asset, sorted', () => {
    expect(
      precacheUrls([
        'assets/pdfium-8k7OGqr9.wasm',
        'index.html',
        'assets/worker-BVc5DJRW.js',
        'manifest.webmanifest',
        'icons/vitela-512.png',
      ]),
    ).toEqual([
      '/',
      '/assets/pdfium-8k7OGqr9.wasm',
      '/assets/worker-BVc5DJRW.js',
      '/icons/vitela-512.png',
      '/manifest.webmanifest',
    ]);
  });

  it('leaves out the worker itself, the host files, the not-found page and source maps', () => {
    expect(
      precacheUrls(['sw.js', '_headers', '_redirects', '404.html', 'assets/index-DkBMLp3E.js.map', 'assets/index-DkBMLp3E.js']),
    ).toEqual(['/assets/index-DkBMLp3E.js']);
  });

  it('never takes a PDF, so a document can never end up in the cache', () => {
    expect(precacheUrls(['sample.pdf', 'assets/x.PDF', 'index.html'])).toEqual(['/']);
  });

  it('reads Windows separators as URL slashes', () => {
    expect(precacheUrls(['assets\\pdf_wasm_bg-BPQ1LRd0.wasm'])).toEqual(['/assets/pdf_wasm_bg-BPQ1LRd0.wasm']);
  });
});

describe('cacheVersion', () => {
  const build = [
    { path: 'index.html', digest: 'a' },
    { path: 'assets/pdfium-8k7OGqr9.wasm', digest: 'b' },
  ];

  it('is the same for the same build, whatever order the files were listed in', () => {
    expect(cacheVersion([...build].reverse(), hash)).toBe(cacheVersion(build, hash));
  });

  it('changes when any precached file changes, so a new deploy gets a new cache', () => {
    const changed = [build[0]!, { path: 'assets/pdfium-8k7OGqr9.wasm', digest: 'c' }];
    expect(cacheVersion(changed, hash)).not.toBe(cacheVersion(build, hash));
  });

  it('changes when a file is added or renamed', () => {
    expect(cacheVersion([...build, { path: 'assets/new-123.js', digest: 'd' }], hash)).not.toBe(cacheVersion(build, hash));
  });

  it('ignores files that are not precached', () => {
    const withHostFiles = [...build, { path: '_headers', digest: 'z' }, { path: 'sw.js', digest: 'y' }];
    expect(cacheVersion(withHostFiles, hash)).toBe(cacheVersion(build, hash));
  });

  it('is a short, name-safe token taken from the digest', () => {
    expect(cacheVersion(build, hash)).toMatch(/^[0-9a-f]{16}$/);
  });
});
