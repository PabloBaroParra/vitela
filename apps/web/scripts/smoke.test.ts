// @vitest-environment node
// scripts/smoke.mjs against a local server that stands in for the deployed
// site: right, it passes; each way the host can serve the app wrong, it fails
// and says where. The server answers with the headers of a `_headers` text it
// is given, so a test breaks the host by editing that text, while the smoke
// test keeps expecting the real public/_headers.

import { readFileSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { resolve } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { headersMiddleware, parseHeaders } from '../src/features/hosting/headers.ts';
import { deploymentUrl, smoke } from './smoke.mjs';

const HEADERS = readFileSync(resolve(import.meta.dirname, '../public/_headers'), 'utf8');

const SHELL =
  '<!doctype html><html><head><script type="module" crossorigin src="/assets/index-abc.js"></script>' +
  '<link rel="stylesheet" crossorigin href="/assets/index-abc.css"><link rel="manifest" href="/manifest.webmanifest">' +
  '</head><body><div id="root"></div></body></html>';

interface Served {
  readonly type: string;
  readonly body: string;
  readonly status?: number;
}

/** The deployed site, served right. */
function site(): Record<string, Served> {
  return {
    '/': { type: 'text/html; charset=utf-8', body: SHELL },
    '/edit': { type: 'text/html; charset=utf-8', body: SHELL },
    '/assets/index-abc.js': { type: 'application/javascript', body: 'new Worker(new URL("/assets/worker-abc.js", import.meta.url))' },
    '/assets/index-abc.css': { type: 'text/css; charset=utf-8', body: 'body{}' },
    '/assets/worker-abc.js': {
      type: 'application/javascript',
      body: 'fetch("assets/pdf_wasm_bg-abc.wasm");fetch("assets/pdfium-abc.wasm");fetch("assets/pdfium-abc.js")',
    },
    '/assets/pdf_wasm_bg-abc.wasm': { type: 'application/wasm', body: '\0asm' },
    '/assets/pdfium-abc.wasm': { type: 'application/wasm', body: '\0asm' },
    '/assets/pdfium-abc.js': { type: 'application/javascript', body: 'var PDFiumModule;' },
    '/sw.js': { type: 'application/javascript', body: 'self.addEventListener("fetch", () => {});' },
    '/manifest.webmanifest': { type: 'application/manifest+json', body: '{"name":"Vitela"}' },
  };
}

const NOT_FOUND: Served = { type: 'text/html; charset=utf-8', body: '<p>This page does not exist.</p>', status: 404 };

let server: Server | undefined;
afterEach(() => new Promise<void>((done) => (server ? server.close(() => done()) : done())));

/** Serves `files` (anything else is `fallback`) under the headers `headersText` gives; resolves to its base URL. */
async function serve(files: Record<string, Served>, { headersText = HEADERS, fallback = NOT_FOUND } = {}): Promise<string> {
  const headers = headersMiddleware(parseHeaders(headersText));
  server = createServer((request, response) => {
    const { pathname } = new URL(request.url ?? '/', 'http://smoke.invalid');
    headers(request, response, () => {
      const served = files[pathname] ?? fallback;
      // A content type _headers sets wins, as on Cloudflare.
      if (!response.hasHeader('Content-Type')) response.setHeader('Content-Type', served.type);
      response.statusCode = served.status ?? 200;
      response.end(served.body);
    });
  });
  await new Promise<void>((listening) => server!.listen(0, '127.0.0.1', () => listening()));
  return `http://127.0.0.1:${(server!.address() as AddressInfo).port}`;
}

const run = (base: string) => smoke(base, { headers: HEADERS, attempts: 1, log: () => {} });

describe('smoke (the deployed site)', () => {
  it('passes a site served right, having checked the shell, a tool route, the hashed assets, the worker and both WASM files', async () => {
    const result = await run(await serve(site()));
    expect(result.failures).toEqual([]);
    expect(result.checked).toEqual(
      expect.arrayContaining([
        '/',
        '/edit',
        '/manifest.webmanifest',
        '/sw.js',
        '/assets/index-abc.js',
        '/assets/index-abc.css',
        '/assets/worker-abc.js',
        '/assets/pdf_wasm_bg-abc.wasm',
        '/assets/pdfium-abc.wasm',
        '/assets/pdfium-abc.js',
      ]),
    );
  });

  it('fails when the page is served another Content-Security-Policy', async () => {
    const loose = HEADERS.replace("script-src 'self'; style-src", "script-src 'self' 'unsafe-inline'; style-src");
    const { failures } = await run(await serve(site(), { headersText: loose }));
    expect(failures.some((failure) => failure.startsWith('/: Content-Security-Policy'))).toBe(true);
  });

  it("fails when the engine worker also gets the page's policy (the `! Content-Security-Policy` detach did not apply)", async () => {
    const undetached = HEADERS.replace(/^\s*! Content-Security-Policy\s*$/m, '');
    const { failures } = await run(await serve(site(), { headersText: undetached }));
    expect(failures.some((failure) => failure.startsWith('/assets/worker-abc.js: Content-Security-Policy'))).toBe(true);
  });

  it('fails when sw.js or the manifest is not served no-cache', async () => {
    const cached = HEADERS.replace(/^\/sw\.js\r?\n\s+Cache-Control: no-cache$/m, '/sw.js\n  X-Unused: 1').replace(
      /(\/manifest\.webmanifest\r?\n\s+Content-Type: application\/manifest\+json\r?\n)\s+Cache-Control: no-cache/,
      '$1',
    );
    const { failures } = await run(await serve(site(), { headersText: cached }));
    expect(failures).toEqual(
      expect.arrayContaining([expect.stringMatching(/^\/sw\.js: Cache-Control/), expect.stringMatching(/^\/manifest\.webmanifest: Cache-Control/)]),
    );
  });

  it('fails when a WASM file is not served as application/wasm', async () => {
    const files = site();
    files['/assets/pdfium-abc.wasm'] = { type: 'application/octet-stream', body: '\0asm' };
    const { failures } = await run(await serve(files));
    expect(failures.some((failure) => failure.startsWith('/assets/pdfium-abc.wasm: Content-Type'))).toBe(true);
  });

  it('fails when an unknown path gets the shell instead of a 404 (a single-page-app fallback)', async () => {
    const { failures } = await run(await serve(site(), { fallback: { type: 'text/html', body: SHELL } }));
    expect(failures.some((failure) => /^\/__vitela-smoke-[^:]+: status 200, expected 404/.test(failure))).toBe(true);
    expect(failures.some((failure) => /^\/assets\/__vitela-smoke-[^:]+\.js: status 200, expected 404/.test(failure))).toBe(true);
  });

  it('fails, naming the path, when a file is missing', async () => {
    const files = site();
    delete files['/sw.js'];
    const { failures } = await run(await serve(files));
    expect(failures).toContain('/sw.js: status 404, expected 200');
  });

  it('fails when no hashed asset can be found in the page', async () => {
    const files = site();
    files['/'] = { type: 'text/html', body: '<div id="root"></div>' };
    const { failures } = await run(await serve(files));
    expect(failures).toContain('/: no /assets/*.js entry script in the page');
  });

  it('fails when the site does not answer at all', async () => {
    const base = await serve(site());
    await new Promise<void>((done) => server!.close(() => done()));
    server = undefined;
    const { failures } = await run(base);
    expect(failures).toHaveLength(1);
    expect(failures[0]).toMatch(/^\/: /);
  });
});

describe('deploymentUrl', () => {
  it("takes the deployment's URL from wrangler's output file", () => {
    const output = [
      '{"type":"wrangler-session","version":1}',
      '{"type":"pages-deploy-detailed","version":1,"url":"https://abc123.vitela.pages.dev","alias":"https://vitela.pages.dev"}',
      '',
    ].join('\n');
    expect(deploymentUrl(output)).toBe('https://abc123.vitela.pages.dev');
  });

  it('has none when wrangler wrote no deployment', () => {
    expect(deploymentUrl('{"type":"wrangler-session"}\nnot json\n')).toBeUndefined();
    expect(deploymentUrl('')).toBeUndefined();
  });
});
