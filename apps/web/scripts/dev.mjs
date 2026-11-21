// `npm run dev`: Vite plus the watch builds of @vitela/core and
// @vitela/viewer, so edits to either package reach the running dev server
// without a restart (Vite watches each package's dist/, which its watcher
// keeps current). `npm run build:packages` runs first, so both dist/ are
// complete before Vite starts.
//
// Both children are `node <script>` (no shell, no npm shim), so Ctrl+C stops
// them on Windows and Linux alike.
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';

const require = createRequire(import.meta.url);
const coreDir = dirname(require.resolve('@vitela/core/package.json'));
const viewerDir = dirname(require.resolve('@vitela/viewer/package.json'));
const viteDir = dirname(require.resolve('vite/package.json'));

const children = [
  spawn(process.execPath, [join(coreDir, 'scripts', 'watch.mjs')], { stdio: 'inherit' }),
  spawn(process.execPath, [join(viewerDir, 'scripts', 'watch.mjs')], { stdio: 'inherit' }),
  spawn(process.execPath, [join(viteDir, require('vite/package.json').bin.vite), ...process.argv.slice(2)], {
    stdio: 'inherit',
  }),
];

let stopping = false;
function stop(code) {
  if (stopping) return;
  stopping = true;
  for (const child of children) child.kill();
  process.exitCode = code;
}
process.on('SIGINT', () => stop(0));
process.on('SIGTERM', () => stop(0));
// Any one dying takes the others down: a dev server with a dead watcher
// would silently serve a stale engine or editor.
for (const child of children) child.on('exit', (code) => stop(code ?? 0));
