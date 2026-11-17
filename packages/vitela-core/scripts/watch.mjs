// `npm run watch`: keeps dist/ current while a host's dev server reads it.
// tsc --watch for both halves (DOM entry + worker; each compiles everything
// once on start), plus the WASM asset copy on start and again whenever
// src/generated/ changes (after `npm run build:wasm`). dist/ is never wiped,
// so a running dev server never sees it disappear.
//
// Every child is `node <script>`, never a shell or an npm shim, so stopping
// this process stops them on Windows and Linux alike.
import { spawn, spawnSync } from 'node:child_process';
import { watch } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const pkgDir = fileURLToPath(new URL('..', import.meta.url));
const require = createRequire(import.meta.url);
const tsc = join(dirname(require.resolve('typescript/package.json')), require('typescript/package.json').bin.tsc);
const PROJECTS = ['tsconfig.build.json', 'tsconfig.build-worker.json'];

function node(args) {
  const result = spawnSync(process.execPath, args, { cwd: pkgDir, stdio: 'inherit' });
  return result.status === 0;
}

function copyAssets() {
  return node(['scripts/write-integrity.mjs']) && node(['scripts/copy-assets.mjs']);
}

if (!copyAssets()) process.exit(1);

const children = PROJECTS.map((project) =>
  spawn(process.execPath, [tsc, '-p', project, '--watch', '--preserveWatchOutput'], { cwd: pkgDir, stdio: 'inherit' }),
);

// A wasm rebuild rewrites several files in a burst; copy once it settles.
let pending;
watch(join(pkgDir, 'src', 'generated'), { recursive: true }, (_event, file) => {
  // write-integrity.mjs writes into src/generated/ too; do not loop on it.
  if (file?.startsWith('integrity.')) return;
  clearTimeout(pending);
  pending = setTimeout(copyAssets, 500);
});

function stop() {
  for (const child of children) child.kill();
  process.exit(0);
}
process.on('SIGINT', stop);
process.on('SIGTERM', stop);
for (const child of children) child.on('exit', (code) => code && stop());
