// `npm run setup:registry -- <version>`: installs that published version of
// @vitela/core and @vitela/viewer from the npm registry, like any consumer,
// on top of this example's own locked dependencies, and copies the sample.
//
// A version just published may not be served everywhere yet, so a "no such
// package/version" answer is retried for up to five minutes. Anything else
// fails at once.
import { readFileSync } from 'node:fs';
import { parseVersion } from '../../../scripts/npm-release/version.mjs';
import { notPropagatedYet, retry } from './retry.mjs';
import { copySample, here, installArgs, orExit, removeInstalled, tryNpm } from './shared.mjs';

const ATTEMPTS = 11;
const DELAY_MS = 30_000;

const version = process.argv[2];
if (!parseVersion(version)) {
  console.error('usage: npm run setup:registry -- <version>   e.g. 0.1.0');
  process.exit(1);
}

const packages = ['@vitela/core', '@vitela/viewer'];
// --prefer-online: a retry must ask the registry again, not npm's cache.
const args = installArgs('--prefer-online', ...packages.map((name) => `${name}@${version}`));

removeInstalled();
const result = await retry(() => tryNpm(args, here('.')), {
  attempts: ATTEMPTS,
  delayMs: DELAY_MS,
  again: notPropagatedYet,
});
orExit(result, args, here('.'));

// What npm really put there, and where it came from.
const { packages: installed } = JSON.parse(readFileSync(here('node_modules/.package-lock.json'), 'utf8'));
for (const name of packages) {
  const entry = installed[`node_modules/${name}`];
  if (entry?.version !== version || !/^https?:/.test(entry.resolved ?? '')) {
    console.error(`${name}: expected ${version} from the registry, got ${entry?.version} from ${entry?.resolved}`);
    process.exit(1);
  }
}

copySample();
console.log(`installed ${packages.join(', ')} ${version} from the registry; public/sample.pdf ready`);
