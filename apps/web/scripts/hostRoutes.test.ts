// @vitest-environment node
// `vite preview` routes like the deployed site: public/_redirects' rewrites
// serve the shell, a file is itself, and anything else is 404.html with a 404
// (Vite's own preview would answer it with the shell, as a single-page app).

import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { PreviewServer } from 'vite';
import { afterAll, describe, expect, it } from 'vitest';
import config from '../vite.config.ts';
import { hostRoutes } from './hostRoutes.ts';

type Middleware = (
  request: { url?: string },
  response: { statusCode: number; setHeader(name: string, value: string): void; end(body?: string): void },
  next: () => void,
) => void;

const root = mkdtempSync(join(tmpdir(), 'vitela-routes-'));
mkdirSync(join(root, 'dist/assets'), { recursive: true });
writeFileSync(join(root, 'dist/_redirects'), '/edit / 200\n');
writeFileSync(join(root, 'dist/index.html'), '<div id="root"></div>');
writeFileSync(join(root, 'dist/404.html'), 'not found');
writeFileSync(join(root, 'dist/assets/index-abc.js'), '');
afterAll(() => rmSync(root, { recursive: true, force: true }));

/** What the preview's routing does with `path`: passes it on (with the URL it ends up as), or answers it. */
function route(path: string) {
  let middleware: Middleware | undefined;
  const server = {
    config: { root, build: { outDir: 'dist' } },
    middlewares: { use: (handler: Middleware) => (middleware = handler) },
  } as unknown as PreviewServer;
  (hostRoutes().configurePreviewServer as (server: PreviewServer) => void)(server);
  const request = { url: path };
  const answered: { status?: number; headers: Record<string, string>; body?: string } = { headers: {} };
  let passed = false;
  middleware!(
    request,
    {
      set statusCode(status: number) {
        answered.status = status;
      },
      get statusCode() {
        return answered.status ?? 200;
      },
      setHeader: (name, value) => void (answered.headers[name] = value),
      end: (body) => void (answered.body = body),
    },
    () => (passed = true),
  );
  return passed ? { passed: request.url } : answered;
}

describe('hostRoutes (vite preview)', () => {
  it("is one of the app's Vite plugins", () => {
    const names = (config.plugins ?? []).flat().map((plugin) => (plugin && typeof plugin === 'object' && 'name' in plugin ? plugin.name : ''));
    expect(names).toContain('vitela-host-routes');
  });

  it('hands the shell for the home page and a rewritten tool route to the static server', () => {
    expect(route('/')).toEqual({ passed: '/' });
    expect(route('/edit?x=1')).toEqual({ passed: '/?x=1' });
  });

  it('hands an existing file to the static server as is', () => {
    expect(route('/assets/index-abc.js')).toEqual({ passed: '/assets/index-abc.js' });
  });

  it('answers any other path with 404.html and a 404', () => {
    for (const path of ['/no-such-page', '/assets/index-old.js', '/assets/', '/../package.json']) {
      expect(route(path), path).toEqual({ status: 404, headers: { 'Content-Type': 'text/html; charset=utf-8' }, body: 'not found' });
    }
  });
});
