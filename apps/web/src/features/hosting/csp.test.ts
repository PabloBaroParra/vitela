// Pins the Content-Security-Policies public/_headers ships (T-265): what the
// page and the engine worker may do, and that nothing looser slips in. The
// same file is what `vite preview` serves (scripts/hostHeaders.test.ts), so
// the e2e suite runs under exactly these.

import { describe, expect, it } from 'vitest';
import file from '../../../public/_headers?raw';
import { headersFor, parseHeaders, parsePolicy } from './headers';

const rules = parseHeaders(file);
const policyOf = (path: string) => headersFor(rules, path)['Content-Security-Policy'] ?? '';
const directives = (path: string) => Object.fromEntries(parsePolicy(policyOf(path)));

/** Paths served as a page: the shell (home and a tool route) and the host's 404. */
const PAGES = ['/', '/index.html', '/edit', '/organize', '/404.html', '/no-such-page'];
/** The engine worker, as Vite names it (dist/assets/worker-<hash>.js). */
const WORKER = '/assets/worker-BVc5DJRW.js';

describe('the page policy', () => {
  it('is exactly the strict policy, on every page', () => {
    for (const path of PAGES) {
      expect(directives(path), path).toEqual({
        'default-src': ["'none'"],
        'script-src': ["'self'"],
        'style-src': ["'self'"],
        'img-src': ["'self'", 'blob:', 'data:'],
        'connect-src': ["'self'"],
        'worker-src': ["'self'"],
        'manifest-src': ["'self'"],
        'frame-ancestors': ["'none'"],
        'base-uri': ["'none'"],
        'form-action': ["'none'"],
        'object-src': ["'none'"],
      });
    }
  });

  it('allows no inline code, no eval and no other origin', () => {
    const policy = policyOf('/');
    expect(policy).not.toMatch(/unsafe-|nonce-|sha\d+-|strict-dynamic|https?:|\*/);
  });

  it('covers the service worker and the other built files with the same policy', () => {
    for (const path of ['/sw.js', '/manifest.webmanifest', '/assets/index-DkBMLp3E.js', '/assets/pdfium-8k7OGqr9.wasm']) {
      expect(policyOf(path), path).toBe(policyOf('/'));
    }
  });

  it('keeps the other security headers on every page', () => {
    expect(headersFor(rules, '/edit')).toMatchObject({
      'X-Content-Type-Options': 'nosniff',
      'Referrer-Policy': 'no-referrer',
      'Permissions-Policy': 'camera=(), microphone=(), geolocation=()',
    });
  });
});

describe('the engine worker policy', () => {
  it("replaces the page's policy (one policy, not both joined)", () => {
    expect(policyOf(WORKER)).not.toContain(',');
    expect(directives(WORKER)).toEqual({
      'default-src': ["'none'"],
      // Both WASM modules, and pdfium's Emscripten glue run as a function body.
      'script-src': ["'self'", "'unsafe-eval'", "'wasm-unsafe-eval'"],
      'connect-src': ["'self'"],
    });
  });

  it('is the only file that may eval', () => {
    expect(policyOf('/assets/index-DkBMLp3E.js')).not.toContain('unsafe-eval');
    expect(policyOf('/sw.js')).not.toContain('unsafe-eval');
  });

  it('keeps the immutable caching of every hashed asset', () => {
    expect(headersFor(rules, WORKER)['Cache-Control']).toBe('public, max-age=31536000, immutable');
  });
});

describe('public/_headers', () => {
  it("stays within Cloudflare Pages' limits: 100 rules, 2000 characters a line", () => {
    expect(rules.length).toBeLessThanOrEqual(100);
    for (const line of file.split(/\r?\n/)) expect(line.length).toBeLessThanOrEqual(2000);
  });
});
