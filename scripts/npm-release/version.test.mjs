import { test } from 'node:test';
import assert from 'node:assert/strict';
import { compareVersions, distTagFor, parseVersion } from './version.mjs';

test('exact semver versions parse, everything else does not', () => {
  for (const ok of ['0.0.0', '0.1.0', '10.20.30', '1.0.0-beta.1', '1.0.0-rc', '1.0.0-0', '1.0.0-x-y.7']) {
    assert.ok(parseVersion(ok), ok);
  }
  for (const bad of [
    '', 'v1.0.0', '1.0', '1.0.0.0', '01.0.0', '1.0.0-', '1.0.0-beta..1', '1.0.0-01', '1.0.0+build', '1.0.0-rc.1+x',
    '^1.0.0', 'latest', ' 1.0.0', '1.0.0\n', undefined,
  ]) {
    assert.equal(parseVersion(bad), null, JSON.stringify(bad));
  }
});

test('versions order as semver says', () => {
  const ordered = [
    '0.0.0', '0.1.0', '0.2.0', '0.10.0', '1.0.0-1', '1.0.0-2', '1.0.0-10', '1.0.0-alpha', '1.0.0-alpha.1',
    '1.0.0-alpha.beta', '1.0.0-beta', '1.0.0-beta.2', '1.0.0-beta.11', '1.0.0-rc.1', '1.0.0', '1.0.1', '2.0.0',
    '99999999999999999999.0.0', '100000000000000000000.0.0',
  ];
  for (const [i, a] of ordered.entries()) {
    for (const [j, b] of ordered.entries()) {
      assert.equal(Math.sign(compareVersions(a, b)), Math.sign(i - j), `${a} vs ${b}`);
    }
  }
});

test('comparing something that is not a version throws', () => {
  assert.throws(() => compareVersions('1.0.0', 'latest'), /not a semver version/);
});

test('a prerelease goes to next, a stable version to latest', () => {
  assert.equal(distTagFor('1.0.0-beta.1'), 'next');
  assert.equal(distTagFor('1.0.0'), 'latest');
});
