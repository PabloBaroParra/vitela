import { test } from 'node:test';
import assert from 'node:assert/strict';
import { releaseCheck } from './check.mjs';

const manifests = (version, { viewer = version, dep = version } = {}) => ({
  core: { version },
  viewer: { version: viewer, dependencies: { '@vitela/core': dep } },
});

test('a stable tag gives the version and the latest dist-tag', () => {
  assert.deepEqual(releaseCheck({ tag: 'npm-v0.1.0', ...manifests('0.1.0') }), { version: '0.1.0', distTag: 'latest' });
});

test('a prerelease tag gives the next dist-tag', () => {
  assert.deepEqual(releaseCheck({ tag: 'npm-v0.2.0-beta.1', ...manifests('0.2.0-beta.1') }), {
    version: '0.2.0-beta.1',
    distTag: 'next',
  });
});

test('without a tag (a rehearsal) the version is the one in the packages, checked the same way', () => {
  assert.deepEqual(releaseCheck(manifests('0.1.0')), { version: '0.1.0', distTag: 'latest' });
  assert.throws(() => releaseCheck(manifests('0.1.0', { viewer: '0.2.0' })), /@vitela\/viewer is 0\.2\.0/);
});

test('a tag that is not npm-v<semver> is refused', () => {
  for (const tag of ['v0.1.0', 'npm-0.1.0', 'npm-v', 'npm-v0.1', 'npm-vv0.1.0', 'xnpm-v0.1.0', '']) {
    assert.throws(() => releaseCheck({ tag, ...manifests('0.1.0') }), /npm-v<semver>/, tag);
  }
});

test('build metadata is refused by name', () => {
  assert.throws(() => releaseCheck({ tag: 'npm-v0.1.0+x', ...manifests('0.1.0+x') }), /build metadata/);
});

test('each package must be at the tagged version', () => {
  const tag = (version, packages) => () => releaseCheck({ tag: `npm-v${version}`, ...packages });
  assert.throws(tag('0.2.0', manifests('0.1.0', { viewer: '0.2.0' })), /@vitela\/core is 0\.1\.0/);
  assert.throws(tag('0.1.0', manifests('0.1.0', { viewer: '0.2.0' })), /@vitela\/viewer is 0\.2\.0/);
});

test('the viewer must depend on exactly that core', () => {
  for (const dep of ['^0.1.0', '0.0.9', '*', 'file:../vitela-core', undefined]) {
    const { core, viewer } = manifests('0.1.0');
    viewer.dependencies = { '@vitela/core': dep };
    assert.throws(() => releaseCheck({ tag: 'npm-v0.1.0', core, viewer }), /depends on @vitela\/core/, dep);
  }
});
