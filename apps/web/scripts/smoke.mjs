// @ts-check
// Post-deploy smoke test (T-268): asks a deployed copy of the web app for the
// files that make it work and checks how the host serves them. Run by
// .github/workflows/web-deploy.yml right after the Cloudflare Pages upload,
// against that deployment's own URL, and by e2e/smoke.spec.ts against
// `vite preview` on every pull request.
//
//   node scripts/smoke.mjs <base-url>
//       Checks the site; exits 1, listing every problem, if any check fails.
//   node scripts/smoke.mjs --deployment-url <wrangler-output-file>
//       Prints the URL of the deployment `wrangler pages deploy` just made
//       (from the ND-JSON file WRANGLER_OUTPUT_FILE_PATH names), or nothing.
//
// What it checks, starting from `/` and following the build's own references
// (so no hashed name is hard-coded):
//   - status 200 and the right Content-Type for `/`, a tool route (`/edit`,
//     the same shell), /manifest.webmanifest, /sw.js, the page's hashed
//     entry script and stylesheet, the engine worker, pdfium's glue and both
//     WASM files (application/wasm, which streaming compilation requires);
//   - every header public/_headers gives each of those paths, read with the
//     same parser `vite preview` serves them with
//     (src/features/hosting/headers.ts): the page's Content-Security-Policy,
//     the worker's own policy (the `! Content-Security-Policy` detach, which
//     only a real Cloudflare deploy can prove), `immutable` under /assets/;
//   - sw.js and the manifest are no-cache, and the worker's policy is not
//     the page's, whatever _headers says;
//   - an unknown path, and a stale hashed asset, get a 404, not the shell.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { headersFor, parseHeaders } from '../src/features/hosting/headers.ts';

/** The headers the site is expected to send: the file the build copies into dist/. */
export const HEADERS_FILE = fileURLToPath(new URL('../public/_headers', import.meta.url));

/** A tool route (public/_redirects): the shell again, by a 200 rewrite. */
const TOOL_ROUTE = '/edit';

const JAVASCRIPT = ['text/javascript', 'application/javascript'];
const HTML = ['text/html'];

/**
 * @typedef {object} SmokeOptions
 * @property {string} [headers] public/_headers' text (default: the file).
 * @property {number} [attempts] Tries for the first request, while a new deployment comes up.
 * @property {number} [retryDelayMs] Wait between those tries.
 * @property {(line: string) => void} [log] Where each checked path is reported.
 */

/**
 * @typedef {object} SmokeResult
 * @property {string[]} checked The paths asked for, in order.
 * @property {string[]} failures One line per problem, "<path>: <what is wrong>".
 */

/** @param {string | null} value */
const mediaType = (value) => (value ?? '').split(';')[0].trim().toLowerCase();

/** @param {string} value */
const normalize = (value) => value.trim().replace(/\s+/g, ' ');

/** @param {number} ms */
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Checks the site at `base`.
 * @param {string} base
 * @param {SmokeOptions} [options]
 * @returns {Promise<SmokeResult>}
 */
export async function smoke(base, options = {}) {
  const { headers = readFileSync(HEADERS_FILE, 'utf8'), attempts = 6, retryDelayMs = 5000, log = console.log } = options;
  const rules = parseHeaders(headers);
  const origin = new URL(base.endsWith('/') ? base : `${base}/`);
  /** @type {string[]} */
  const checked = [];
  /** @type {string[]} */
  const failures = [];

  /** @param {string} path */
  async function get(path) {
    try {
      // Manual: a redirect is reported as one, not followed into a 200.
      const response = await fetch(new URL(path.slice(1), origin), { redirect: 'manual', headers: { 'Cache-Control': 'no-cache' } });
      return { response, body: await response.text() };
    } catch (error) {
      return { error: error instanceof Error ? (error.cause instanceof Error ? `${error.message}: ${error.cause.message}` : error.message) : String(error) };
    }
  }

  /**
   * Asks for `path` and checks its status, type and _headers headers.
   * Resolves to the body when it was served with `status`.
   * @param {string} path
   * @param {{ status?: number, types?: string[], tries?: number }} [expect]
   */
  async function check(path, { status = 200, types, tries = 1 } = {}) {
    checked.push(path);
    let got = await get(path);
    for (let attempt = 1; attempt < tries && ('error' in got || got.response.status >= 500 || got.response.status === 404); attempt++) {
      await wait(retryDelayMs);
      got = await get(path);
    }
    /** @param {string} problem */
    const fail = (problem) => failures.push(`${path}: ${problem}`);
    if ('error' in got) {
      fail(`no answer (${got.error})`);
      log(`FAIL ${path}`);
      return { body: undefined, failed: new Set() };
    }
    const { response, body } = got;
    const before = failures.length;
    if (response.status !== status) {
      fail(`status ${response.status}, expected ${status}`);
      log(`FAIL ${response.status} ${path}`);
      return { body: undefined, failed: new Set(), response };
    }
    /** @type {Set<string>} */
    const failed = new Set();
    const type = response.headers.get('content-type');
    if (types && !types.includes(mediaType(type))) {
      fail(`Content-Type ${JSON.stringify(type)}, expected ${types.join(' or ')}`);
      failed.add('content-type');
    }
    if (status === 200) {
      for (const [name, value] of Object.entries(headersFor(rules, path))) {
        const key = name.toLowerCase();
        const actual = response.headers.get(key);
        const same = key === 'content-type' ? mediaType(actual) === mediaType(value) : actual !== null && normalize(actual) === normalize(value);
        if (same || failed.has(key)) continue;
        fail(actual === null ? `${name} missing, expected ${JSON.stringify(value)}` : `${name} ${JSON.stringify(actual)}, expected ${JSON.stringify(value)}`);
        failed.add(key);
      }
    }
    log(`${failures.length === before ? 'ok  ' : 'FAIL'} ${response.status} ${path}`);
    return { body, failed, response };
  }

  /**
   * Checks that `path` is served no-cache, whatever _headers says.
   * @param {string} path
   * @param {{ failed: Set<string>, response?: Response }} result
   */
  function requireNoCache(path, { failed, response }) {
    if (!response || failed.has('cache-control')) return;
    const value = response.headers.get('cache-control');
    if (!(value ?? '').split(',').some((directive) => directive.trim().toLowerCase() === 'no-cache')) {
      failures.push(`${path}: Cache-Control ${JSON.stringify(value)}, must be no-cache`);
    }
  }

  // The shell, retried while a fresh deployment comes up.
  const home = await check('/', { types: HTML, tries: attempts });
  // No answer at all: nothing else will answer either.
  if (!home.response) return { checked, failures };
  const pageCsp = home.response?.headers.get('content-security-policy') ?? null;
  if (home.body !== undefined && !pageCsp && !home.failed.has('content-security-policy')) {
    failures.push('/: no Content-Security-Policy');
  }
  await check(TOOL_ROUTE, { types: HTML });

  const manifest = await check('/manifest.webmanifest', { types: ['application/manifest+json'] });
  requireNoCache('/manifest.webmanifest', manifest);
  const worker = await check('/sw.js', { types: JAVASCRIPT });
  requireNoCache('/sw.js', worker);

  // The hashed assets, discovered from the page and the bundles.
  const shell = home.body ?? '';
  const entry = /<script\b[^>]*\bsrc="(\/assets\/[^"]+\.js)"/.exec(shell)?.[1];
  const stylesheet = /<link\b[^>]*\bhref="(\/assets\/[^"]+\.css)"/.exec(shell)?.[1];
  if (home.body !== undefined && !entry) failures.push('/: no /assets/*.js entry script in the page');
  if (stylesheet) await check(stylesheet, { types: ['text/css'] });
  if (entry) {
    const script = await check(entry, { types: JAVASCRIPT });
    const engine = assetsIn(script.body).find((path) => /^\/assets\/worker-[^/]+\.js$/.test(path));
    if (script.body !== undefined && !engine) failures.push(`${entry}: no engine worker (/assets/worker-*.js) referenced`);
    if (engine) {
      const served = await check(engine, { types: JAVASCRIPT });
      const policy = served.response?.headers.get('content-security-policy') ?? null;
      if (served.response?.status === 200 && !served.failed.has('content-security-policy')) {
        if (!policy) failures.push(`${engine}: no Content-Security-Policy of its own`);
        else if (pageCsp && normalize(policy).includes(normalize(pageCsp))) {
          failures.push(`${engine}: Content-Security-Policy includes the page's (the "! Content-Security-Policy" detach did not apply)`);
        }
      }
      const loaded = assetsIn(served.body).filter((path) => path !== engine);
      const wasm = loaded.filter((path) => path.endsWith('.wasm'));
      if (served.body !== undefined && wasm.length === 0) failures.push(`${engine}: no /assets/*.wasm referenced`);
      for (const path of loaded) await check(path, { types: path.endsWith('.wasm') ? ['application/wasm'] : JAVASCRIPT });
    }
  }

  // Unknown paths are the host's 404, never the shell (no single-page-app
  // fallback): a stale hashed asset must not get HTML cached as immutable.
  const nonce = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
  for (const path of [`/__vitela-smoke-${nonce}`, `/assets/__vitela-smoke-${nonce}.js`]) {
    const missing = await check(path, { status: 404 });
    if (entry && missing.body?.includes(entry)) {
      failures.push(`${path}: the 404 is the app shell`);
    }
  }
  return { checked, failures };
}

/**
 * The /assets/ files a bundle references, in order, each once.
 * @param {string | undefined} body
 */
function assetsIn(body) {
  return [...new Set([...(body ?? '').matchAll(/assets\/([\w-]+(?:\.[\w-]+)*\.(?:js|wasm))/g)].map((match) => `/assets/${match[1]}`))];
}

/**
 * The URL of the deployment `wrangler pages deploy` made, from its output
 * file (ND-JSON; the `pages-deploy-detailed` entry).
 * @param {string} text
 * @returns {string | undefined}
 */
export function deploymentUrl(text) {
  let url;
  for (const line of text.split(/\r?\n/)) {
    try {
      const entry = JSON.parse(line);
      if (entry?.type === 'pages-deploy-detailed' && typeof entry.url === 'string') url = entry.url;
    } catch {
      // Not an entry.
    }
  }
  return url;
}

/** @param {string[]} argv */
async function main(argv) {
  if (argv[0] === '--deployment-url') {
    if (!argv[1]) throw new Error('usage: smoke.mjs --deployment-url <wrangler-output-file>');
    let text = '';
    try {
      text = readFileSync(argv[1], 'utf8');
    } catch {
      // No output file: no URL.
    }
    const url = deploymentUrl(text);
    if (url) console.log(url);
    return 0;
  }
  const [base] = argv;
  if (!base) throw new Error('usage: smoke.mjs <base-url> | --deployment-url <wrangler-output-file>');
  console.log(`smoke test of ${base}`);
  const { failures } = await smoke(base);
  if (failures.length === 0) {
    console.log('all checks passed');
    return 0;
  }
  // On GitHub Actions, each problem is an annotation on the run.
  const prefix = process.env.GITHUB_ACTIONS ? '::error::' : '';
  for (const failure of failures) console.error(`${prefix}smoke: ${failure}`);
  return 1;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main(process.argv.slice(2)).then(
    (code) => (process.exitCode = code),
    (error) => {
      console.error(error instanceof Error ? error.message : error);
      process.exitCode = 2;
    },
  );
}
