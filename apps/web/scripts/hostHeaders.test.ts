// @vitest-environment node
// `vite preview` and the deployed site send the same headers: the preview
// plugin is in the app's Vite config and serves the built copy of
// public/_headers through the same reading of it Cloudflare's rules get.

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { PreviewServer } from 'vite';
import { describe, expect, it } from 'vitest';
import config from '../vite.config.ts';
import { headersFor, parseHeaders } from '../src/features/hosting/headers.ts';
import { hostHeaders } from './hostHeaders.ts';

const ROOT = resolve(import.meta.dirname, '..');
const PATHS = ['/', '/edit', '/no-such-page', '/sw.js', '/manifest.webmanifest', '/assets/worker-BVc5DJRW.js', '/assets/index-x.js'];

type Middleware = (request: { url?: string }, response: { setHeader(name: string, value: string): void }, next: () => void) => void;

/** The headers the preview server answers `path` with, the build's output dir being `outDir`. */
function previewHeaders(outDir: string, path: string): Record<string, string> {
  let middleware: Middleware | undefined;
  const server = {
    config: { root: ROOT, build: { outDir } },
    middlewares: { use: (handler: Middleware) => (middleware = handler) },
  } as unknown as PreviewServer;
  const plugin = hostHeaders();
  (plugin.configurePreviewServer as (server: PreviewServer) => void)(server);
  const headers: Record<string, string> = {};
  let handed = false;
  middleware!({ url: path }, { setHeader: (name, value) => void (headers[name] = value) }, () => (handed = true));
  expect(handed).toBe(true);
  return headers;
}

describe('hostHeaders (vite preview)', () => {
  it("is one of the app's Vite plugins", () => {
    const names = (config.plugins ?? []).flat().map((plugin) => (plugin && typeof plugin === 'object' && 'name' in plugin ? plugin.name : ''));
    expect(names).toContain('vitela-host-headers');
  });

  it('serves each path with exactly the headers public/_headers gives it on the deployed site', () => {
    // The build copies public/ into dist/ as is; read here from public/.
    const rules = parseHeaders(readFileSync(resolve(ROOT, 'public/_headers'), 'utf8'));
    for (const path of PATHS) {
      const served = previewHeaders('public', path);
      expect(served, path).toEqual(headersFor(rules, path));
      expect(served['Content-Security-Policy'], path).toBeTruthy();
    }
  });
});
