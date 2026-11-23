// Publishes the two tarballs web.yml tested, for .github/workflows/npm-release.yml:
// @vitela/core first, then @vitela/viewer, which depends on it.
//
// Usage: node scripts/npm-release/publish.mjs <dir of tarballs> <version> <dist-tag>
//
// Safe to run again after a partial failure: a package already in the registry
// with these exact bytes is skipped. Both packages are checked before either
// is published.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { compareVersions, distTagFor } from './version.mjs';

// In publish order. The file names are the ones `npm pack` gives them.
const PACKAGES = [
  { name: '@vitela/core', file: 'vitela-core' },
  { name: '@vitela/viewer', file: 'vitela-viewer' },
];

/** The two tarballs of `version` in `dir`, in publish order; throws unless they are all there is. */
export function findTarballs(dir, version) {
  const expected = PACKAGES.map(({ file }) => `${file}-${version}.tgz`);
  const found = readdirSync(dir).sort();
  if (found.join() !== [...expected].sort().join()) {
    throw new Error(`${dir}: expected exactly ${expected.join(', ')}, found ${found.join(', ') || 'nothing'}`);
  }
  return PACKAGES.map(({ name }, i) => {
    const path = resolve(dir, expected[i]);
    return { name, path, integrity: `sha512-${createHash('sha512').update(readFileSync(path)).digest('base64')}` };
  });
}

/**
 * `npm view <spec> <field> --json`, or null when the registry says E404 (no
 * such package or version). Any other outcome that is not a `valid` answer
 * throws: an unreachable registry must not read as "not published".
 */
function view(npm, spec, field, valid) {
  const { status, stdout, stderr } = npm(['view', spec, field, '--json']);
  let answer;
  try {
    answer = JSON.parse(stdout);
  } catch {
    // Not an answer; reported below.
  }
  if (status === 0 && valid(answer)) return answer;
  if (status === 1 && answer?.error?.code === 'E404') return null;
  throw new Error(`npm view ${spec} ${field} gave no usable answer (exit ${status})\n${stdout}${stderr}`);
}

/** 'publish' or 'skip' for one package; throws when it must not be published. */
export function decide({ name, version, distTag, integrity, published, latest }) {
  if (published === integrity) return 'skip';
  if (published !== null) {
    throw new Error(
      `${name}@${version} is already published with different bytes (registry ${published}, tested ${integrity})`,
    );
  }
  if (distTag === 'latest' && latest !== null && compareVersions(version, latest) < 0) {
    throw new Error(`publishing would move ${name}'s latest back from ${latest} to ${version}`);
  }
  return 'publish';
}

/** Checks every tarball against the registry, then publishes what is missing. `npm(args)` runs npm. */
export function release({ tarballs, version, distTag, npm, log }) {
  const expectedTag = distTagFor(version);
  if (distTag !== expectedTag) throw new Error(`${version} is published as ${expectedTag}, not ${distTag}`);

  const plan = tarballs.map(({ name, path, integrity }) => {
    const published = view(npm, `${name}@${version}`, 'dist.integrity', (answer) => typeof answer === 'string');
    const tags = view(npm, name, 'dist-tags', (answer) => answer?.constructor === Object);
    const action = decide({ name, version, distTag, integrity, published, latest: tags?.latest ?? null });
    return { name, path, action };
  });

  for (const { name, path, action } of plan) {
    if (action === 'skip') {
      log(`${name}@${version} is already published with these bytes: skipped`);
      continue;
    }
    log(`publishing ${name}@${version} as ${distTag}`);
    const { status } = npm(['publish', path, '--provenance', '--access', 'public', '--tag', distTag]);
    if (status !== 0) throw new Error(`npm publish ${name}@${version} failed (exit ${status})`);
  }
}

/** Runs npm without a shell: its own CLI script when run from npm (npm is npm.cmd on Windows), else `npm`. */
function runNpm(args) {
  const cli = process.env.npm_execpath;
  const [command, prefix] = cli ? [process.execPath, [cli]] : ['npm', []];
  const stdio = args[0] === 'publish' ? 'inherit' : ['ignore', 'pipe', 'pipe'];
  const { status, stdout, stderr, error } = spawnSync(command, [...prefix, ...args], { encoding: 'utf8', stdio });
  if (error) throw error;
  return { status, stdout: stdout ?? '', stderr: stderr ?? '' };
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [dir, version, distTag] = process.argv.slice(2);
  try {
    if (!dir || !version || !distTag) throw new Error('usage: publish.mjs <dir of tarballs> <version> <dist-tag>');
    release({ tarballs: findTarballs(dir, version), version, distTag, npm: runNpm, log: console.log });
  } catch (error) {
    console.error(`npm release: ${error.message}`);
    process.exit(1);
  }
}
