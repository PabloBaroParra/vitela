// `npm run setup`: installs @vitela/core and @vitela/viewer the way a
// consumer gets them from npm, as packed tarballs, never as workspace links.
//
// 1. `npm pack` both packages into .packs/ (their `prepack` cleans and builds
//    dist/; @vitela/core's WASM must already be built: `npm run build:wasm`).
// 2. `npm install --no-save` the two tarballs on top of this example's own
//    locked dependencies (React, Vite, Playwright): package.json and the
//    lockfile keep no file: reference, whose integrity would change on
//    every build.
// 3. Copies a committed fixture to public/sample.pdf, the URL the pages open.
//
// The packages are not published yet (T-248); once they are, a consumer runs
// `npm install @vitela/viewer` instead.
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const here = (path) => fileURLToPath(new URL(`../${path}`, import.meta.url));
const repo = (path) => fileURLToPath(new URL(`../../../${path}`, import.meta.url));

/** Runs npm with `args` in `cwd`; exits on failure. Returns its stdout. */
function npm(args, cwd) {
  // Run as `npm run setup`, npm names its own CLI script: run that with this
  // Node, no shell (npm is npm.cmd on Windows, which only a shell runs).
  const cli = process.env.npm_execpath;
  if (!cli) {
    console.error('run this as `npm run setup`');
    process.exit(1);
  }
  const result = spawnSync(process.execPath, [cli, ...args], { cwd, encoding: 'utf8', stdio: ['inherit', 'pipe', 'inherit'] });
  if (result.status !== 0) {
    process.stdout.write(result.stdout ?? '');
    console.error(`npm ${args.join(' ')} failed in ${cwd}`);
    process.exit(result.status ?? 1);
  }
  return result.stdout;
}

const packs = here('.packs');
rmSync(packs, { recursive: true, force: true });
mkdirSync(packs, { recursive: true });

const tarballs = [];
for (const pkg of ['packages/vitela-core', 'packages/vitela-viewer']) {
  const out = npm(['pack', '--json', '--pack-destination', packs], repo(pkg));
  // Lifecycle scripts may print before the JSON report.
  const [{ filename }] = JSON.parse(out.slice(out.indexOf('[')));
  tarballs.push(`./.packs/${filename}`);
}

npm(['install', '--no-save', '--no-audit', '--no-fund', ...tarballs], here('.'));

mkdirSync(here('public'), { recursive: true });
copyFileSync(repo('tests/fixtures/compress/vector_only.pdf'), here('public/sample.pdf'));
console.log(`installed ${tarballs.join(', ')}; public/sample.pdf ready`);
