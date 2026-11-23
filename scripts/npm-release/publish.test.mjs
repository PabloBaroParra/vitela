import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { decide, findTarballs, release } from './publish.mjs';

const sri = (text) => `sha512-${createHash('sha512').update(text).digest('base64')}`;
const E404 = { status: 1, stdout: JSON.stringify({ error: { code: 'E404', summary: 'Not Found' } }), stderr: '' };
const json = (value) => ({ status: 0, stdout: JSON.stringify(value), stderr: '' });

const CORE = { name: '@vitela/core', path: '/packs/core.tgz', integrity: sri('core') };
const VIEWER = { name: '@vitela/viewer', path: '/packs/viewer.tgz', integrity: sri('viewer') };

/** A fake npm over `{ 'view <spec> <field>': result }`; unknown views are E404, publishes succeed. */
function fakeNpm(answers = {}) {
  const published = [];
  const npm = (args) => {
    if (args[0] === 'publish') {
      published.push(args);
      return answers[`publish ${args[1]}`] ?? { status: 0, stdout: '', stderr: '' };
    }
    assert.equal(args.at(-1), '--json');
    return answers[args.slice(0, 3).join(' ')] ?? E404;
  };
  return { npm, published };
}
const run = (answers, options = {}) => {
  const { npm, published } = fakeNpm(answers);
  const input = { tarballs: [CORE, VIEWER], version: '0.2.0', distTag: 'latest', log: () => {}, npm, ...options };
  return { go: () => release(input), published };
};
const publishArgs = (tarball, tag = 'latest') => ['publish', tarball.path, '--provenance', '--access', 'public', '--tag', tag];

test('nothing in the registry: core then viewer, with provenance, public, tagged', () => {
  const { go, published } = run();
  go();
  assert.deepEqual(published, [publishArgs(CORE), publishArgs(VIEWER)]);
});

test('over the 0.0.0 bootstrap placeholder it publishes', () => {
  const { go, published } = run({
    'view @vitela/core dist-tags': json({ latest: '0.0.0' }),
    'view @vitela/viewer dist-tags': json({ latest: '0.0.0' }),
  });
  go();
  assert.equal(published.length, 2);
});

test('a re-run after a partial failure skips what is already there with the same bytes', () => {
  const { go, published } = run({
    'view @vitela/core@0.2.0 dist.integrity': json(CORE.integrity),
    'view @vitela/core dist-tags': json({ latest: '0.2.0' }),
  });
  go();
  assert.deepEqual(published, [publishArgs(VIEWER)]);
});

test('both already published: nothing to do, even when latest has moved past it', () => {
  const { go, published } = run({
    'view @vitela/core@0.2.0 dist.integrity': json(CORE.integrity),
    'view @vitela/viewer@0.2.0 dist.integrity': json(VIEWER.integrity),
    'view @vitela/core dist-tags': json({ latest: '0.3.0' }),
    'view @vitela/viewer dist-tags': json({ latest: '0.3.0' }),
  });
  go();
  assert.deepEqual(published, []);
});

test('the same version with other bytes fails before anything is published', () => {
  const { go, published } = run({ 'view @vitela/viewer@0.2.0 dist.integrity': json(sri('other')) });
  assert.throws(go, /@vitela\/viewer@0\.2\.0 is already published with different bytes/);
  assert.deepEqual(published, []);
});

test('a registry error that is not E404 is never taken for "not published"', () => {
  const failures = [
    { status: 1, stdout: JSON.stringify({ error: { code: 'E500' } }), stderr: '' },
    { status: 1, stdout: JSON.stringify({ error: { code: 'ETIMEDOUT' } }), stderr: '' },
    { status: 1, stdout: '', stderr: 'npm error code E404' },
    { status: 1, stdout: 'not json', stderr: 'boom' },
    { status: null, stdout: '', stderr: '' },
    { status: 0, stdout: '', stderr: '' },
    { status: 0, stdout: 'not json', stderr: '' },
    { status: 0, stdout: '["sha512-a","sha512-b"]', stderr: '' },
    { status: 0, stdout: 'null', stderr: '' },
  ];
  for (const failure of failures) {
    const { go, published } = run({ 'view @vitela/viewer@0.2.0 dist.integrity': failure });
    assert.throws(go, /npm view @vitela\/viewer@0\.2\.0 dist\.integrity/, JSON.stringify(failure));
    assert.deepEqual(published, []);
  }
});

test('a dist-tags error is not taken for "no latest" either', () => {
  for (const failure of [{ status: 1, stdout: '{"error":{"code":"E503"}}', stderr: '' }, json('latest'), json([])]) {
    const { go, published } = run({ 'view @vitela/core dist-tags': failure });
    assert.throws(go, /npm view @vitela\/core dist-tags/);
    assert.deepEqual(published, []);
  }
});

test('latest never goes backwards, and neither package is published when one would', () => {
  const { go, published } = run({ 'view @vitela/viewer dist-tags': json({ latest: '0.10.0' }) });
  assert.throws(go, /would move @vitela\/viewer's latest back from 0\.10\.0 to 0\.2\.0/);
  assert.deepEqual(published, []);
});

test('a prerelease goes to next whatever latest is', () => {
  const answers = { 'view @vitela/core dist-tags': json({ latest: '9.0.0' }) };
  const { go, published } = run(answers, { version: '0.2.0-beta.1', distTag: 'next' });
  go();
  assert.deepEqual(published, [publishArgs(CORE, 'next'), publishArgs(VIEWER, 'next')]);
});

test('a dist-tag that does not match the version is refused', () => {
  assert.throws(run({}, { distTag: 'next' }).go, /0\.2\.0 is published as latest, not next/);
  assert.throws(run({}, { version: '0.2.0-rc.1' }).go, /published as next, not latest/);
  assert.throws(run({}, { version: 'latest' }).go, /not a semver version/);
});

test('a failed core publish stops before the viewer', () => {
  const { go, published } = run({ [`publish ${CORE.path}`]: { status: 1, stdout: '', stderr: '' } });
  assert.throws(go, /npm publish @vitela\/core@0\.2\.0 failed/);
  assert.deepEqual(published, [publishArgs(CORE)]);
});

test('decide is pure: publish or skip', () => {
  const input = { name: '@vitela/core', version: '0.2.0', distTag: 'latest', integrity: 'sha512-a', latest: '0.1.0' };
  assert.equal(decide({ ...input, published: null }), 'publish');
  assert.equal(decide({ ...input, published: 'sha512-a' }), 'skip');
});

test('findTarballs takes exactly the two tarballs of that version, core first, with their sha512', () => {
  const dir = mkdtempSync(join(tmpdir(), 'npm-release-'));
  try {
    writeFileSync(join(dir, 'vitela-viewer-0.2.0.tgz'), 'viewer');
    assert.throws(() => findTarballs(dir, '0.2.0'), /expected exactly vitela-core-0\.2\.0\.tgz, vitela-viewer-0\.2\.0\.tgz/);
    writeFileSync(join(dir, 'vitela-core-0.2.0.tgz'), 'core');
    assert.deepEqual(
      findTarballs(dir, '0.2.0').map(({ name, integrity }) => ({ name, integrity })),
      [{ name: '@vitela/core', integrity: sri('core') }, { name: '@vitela/viewer', integrity: sri('viewer') }],
    );
    writeFileSync(join(dir, 'vitela-core-0.1.0.tgz'), 'old');
    assert.throws(() => findTarballs(dir, '0.2.0'), /expected exactly/);
  } finally {
    rmSync(dir, { recursive: true });
  }
});
