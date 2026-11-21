// `npm run watch`: keeps dist/ current while a host's dev server reads it.
// tsc --watch for the TypeScript (compiles everything once on start), plus
// the stylesheet copy on start and again whenever src/styles/ changes.
// dist/ is never wiped, so a running dev server never sees it disappear.
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

function copyAssets() {
  return spawnSync(process.execPath, ['scripts/copy-assets.mjs'], { cwd: pkgDir, stdio: 'inherit' }).status === 0;
}

if (!copyAssets()) process.exit(1);

const child = spawn(process.execPath, [tsc, '-p', 'tsconfig.build.json', '--watch', '--preserveWatchOutput'], {
  cwd: pkgDir,
  stdio: 'inherit',
});

// An editor save can write a file more than once; copy once it settles.
let pending;
watch(join(pkgDir, 'src', 'styles'), () => {
  clearTimeout(pending);
  pending = setTimeout(copyAssets, 200);
});

function stop() {
  child.kill();
  process.exit(0);
}
process.on('SIGINT', stop);
process.on('SIGTERM', stop);
child.on('exit', (code) => code && stop());
