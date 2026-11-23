// What .github/workflows/npm-release.yml checks before it spends a test minute:
// the tag names the version both packages carry, and the viewer depends on
// exactly that core.
//
// Usage: node scripts/npm-release/check.mjs [npm-v<semver>]
// Prints `version=…` and `dist-tag=…` (the $GITHUB_OUTPUT format). Without a
// tag (a rehearsal) it checks the packages against each other.
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { distTagFor, parseVersion } from './version.mjs';

const TAG_PREFIX = 'npm-v';

/** `{ version, distTag }` of the release, from the packages' manifests; throws when they do not add up. */
export function releaseCheck({ tag, core, viewer }) {
  const name = tag ?? `${TAG_PREFIX}${core.version}`;
  const version = name.startsWith(TAG_PREFIX) ? name.slice(TAG_PREFIX.length) : '';
  if (version.includes('+')) throw new Error(`'${name}': a version with build metadata cannot be released`);
  if (!parseVersion(version)) throw new Error(`'${name}' is not npm-v<semver>`);

  for (const [pkg, manifest] of [['@vitela/core', core], ['@vitela/viewer', viewer]]) {
    if (manifest.version !== version) throw new Error(`${name}: ${pkg} is ${manifest.version}, not ${version}`);
  }
  const dependency = viewer.dependencies?.['@vitela/core'];
  if (dependency !== version) {
    throw new Error(`${name}: @vitela/viewer depends on @vitela/core ${dependency}, not exactly ${version}`);
  }
  return { version, distTag: distTagFor(version) };
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const manifest = (dir) =>
    JSON.parse(readFileSync(new URL(`../../packages/${dir}/package.json`, import.meta.url), 'utf8'));
  try {
    const { version, distTag } = releaseCheck({
      tag: process.argv[2],
      core: manifest('vitela-core'),
      viewer: manifest('vitela-viewer'),
    });
    console.log(`version=${version}\ndist-tag=${distTag}`);
  } catch (error) {
    console.error(`npm release check: ${error.message}`);
    process.exit(1);
  }
}
