// The post-deploy smoke test (T-268, scripts/smoke.mjs) against `vite
// preview`, so every pull request runs it on the build it ships: the same
// checks web-deploy.yml runs against the Cloudflare deployment. Preview serves
// public/_headers (scripts/hostHeaders.ts) and routes like Pages
// (scripts/hostRoutes.ts), so a check that fails here would fail there.

import { expect, test } from '@playwright/test';
import { smoke } from '../scripts/smoke.mjs';

test('the production build passes the post-deploy smoke test', async ({ baseURL, browserName }) => {
  // Plain HTTP from Node, no browser: one run is enough.
  test.skip(browserName !== 'chromium', 'HTTP checks only; run once, in the Chromium project');
  const { checked, failures } = await smoke(baseURL!, { attempts: 1, log: () => {} });
  expect(failures).toEqual([]);
  // It found the hashed assets: the entry script, the worker and both WASM files.
  expect(checked.filter((path) => /^\/assets\/index-[^/]+\.js$/.test(path))).toHaveLength(1);
  expect(checked.filter((path) => /^\/assets\/worker-[^/]+\.js$/.test(path))).toHaveLength(1);
  expect(checked.filter((path) => path.endsWith('.wasm'))).toHaveLength(2);
});
