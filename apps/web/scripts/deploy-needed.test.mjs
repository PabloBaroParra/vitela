// `node --test scripts/deploy-needed.test.mjs`
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { engineCrateDirs, deployNeeded } from './deploy-needed.mjs';

const root = '/repo/';
const pkg = (name, dir, source = null) => ({
  id: `${name}-id`,
  name,
  source,
  manifest_path: `${root}${dir}Cargo.toml`,
});
const node = (name, deps) => ({
  id: `${name}-id`,
  deps: deps.map(([dep, kind]) => ({ pkg: `${dep}-id`, dep_kinds: [{ kind }] })),
});

const metadata = {
  packages: [
    pkg('pdf-wasm', 'core/pdf-wasm/'),
    pkg('pdf-save', 'core/pdf-save/'),
    pkg('pdf-document', 'core/pdf-document/'),
    pkg('lopdf', 'vendor/lopdf-0.45.0/'),
    pkg('gen-fixtures', 'tests/gen-fixtures/'),
    pkg('pdf-ffi', 'core/pdf-ffi/'),
    pkg('serde', 'registry/serde/', 'registry+https://github.com/rust-lang/crates.io-index'),
  ],
  resolve: {
    nodes: [
      node('pdf-wasm', [['pdf-save', null], ['gen-fixtures', 'dev'], ['serde', null]]),
      node('pdf-save', [['pdf-document', null], ['lopdf', 'build']]),
      node('pdf-document', [['pdf-save', null]]), // a cycle must not loop
      node('lopdf', []),
      node('gen-fixtures', [['pdf-ffi', null]]),
      node('pdf-ffi', []),
      node('serde', []),
    ],
  },
};

test('the engine closure follows normal and build deps, skips dev deps and registry crates', () => {
  assert.deepEqual(engineCrateDirs(metadata, root).sort(), [
    'core/pdf-document/',
    'core/pdf-save/',
    'core/pdf-wasm/',
    'vendor/lopdf-0.45.0/',
  ]);
});

test('a Windows manifest path is normalised before it is made relative', () => {
  const windows = {
    packages: [{ id: 'w', name: 'pdf-wasm', source: null, manifest_path: 'D:\\repo\\core\\pdf-wasm\\Cargo.toml' }],
    resolve: { nodes: [{ id: 'w', deps: [] }] },
  };
  assert.deepEqual(engineCrateDirs(windows, 'D:/repo/'), ['core/pdf-wasm/']);
});

const dirs = ['core/pdf-wasm/', 'core/pdf-save/', 'vendor/lopdf-0.45.0/'];

test('a change to the web or its packages deploys', () => {
  assert.equal(deployNeeded(['apps/web/src/main.tsx'], dirs), true);
  assert.equal(deployNeeded(['packages/vitela-core/src/index.ts'], dirs), true);
  assert.equal(deployNeeded(['package-lock.json'], dirs), true);
  assert.equal(deployNeeded(['.github/workflows/web-deploy.yml'], dirs), true);
});

test('a change to a crate the engine is built from deploys', () => {
  assert.equal(deployNeeded(['core/pdf-save/src/checkpoint.rs'], dirs), true);
  assert.equal(deployNeeded(['vendor/lopdf-0.45.0/src/lib.rs'], dirs), true);
  assert.equal(deployNeeded(['Cargo.lock'], dirs), true);
  assert.equal(deployNeeded(['Cargo.toml'], dirs), true);
  assert.equal(deployNeeded(['.cargo/config.toml'], dirs), true);
  assert.equal(deployNeeded(['rust-toolchain'], dirs), true);
  assert.equal(deployNeeded(['rust-toolchain.toml'], dirs), true);
});

test('a change only to native shells or crates the web does not use is skipped', () => {
  assert.equal(deployNeeded(['core/pdf-ffi/src/lib.rs', 'apps/windows/App.xaml', 'docs/batch-web.md'], dirs), false);
  assert.equal(deployNeeded(['apps/web-other/x'], dirs), false, 'a sibling folder is not apps/web');
  assert.equal(deployNeeded(['core/pdf-save-extra/src/lib.rs'], dirs), false, 'prefix is a directory, not a name');
  assert.equal(deployNeeded([], dirs), false);
});
