// What `npm run setup` and `npm run setup:registry` share: running npm, the
// install on top of the locked dependencies, and the sample the pages open.
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export const here = (path) => fileURLToPath(new URL(`../${path}`, import.meta.url));
export const repo = (path) => fileURLToPath(new URL(`../../../${path}`, import.meta.url));

/** Runs npm with `args` in `cwd`: `{ status, stdout, stderr }`. */
export function tryNpm(args, cwd) {
  // Run as `npm run setup`, npm names its own CLI script: run that with this
  // Node, no shell (npm is npm.cmd on Windows, which only a shell runs).
  const cli = process.env.npm_execpath;
  if (!cli) {
    console.error('run this with `npm run`');
    process.exit(1);
  }
  const result = spawnSync(process.execPath, [cli, ...args], { cwd, encoding: 'utf8', stdio: ['inherit', 'pipe', 'pipe'] });
  process.stderr.write(result.stderr ?? '');
  return { status: result.status, stdout: result.stdout ?? '', stderr: result.stderr ?? '' };
}

/** `result`'s stdout; exits when that npm run failed. */
export function orExit(result, args, cwd) {
  if (result.status !== 0) {
    process.stdout.write(result.stdout);
    console.error(`npm ${args.join(' ')} failed in ${cwd}`);
    process.exit(result.status ?? 1);
  }
  return result.stdout;
}

/** Runs npm with `args` in `cwd`; exits on failure. Returns its stdout. */
export const npm = (args, cwd) => orExit(tryNpm(args, cwd), args, cwd);

/**
 * `npm install` arguments that put `specs` on top of the locked dependencies:
 * package.json and the lockfile stay as committed.
 */
export const installArgs = (...specs) => ['install', '--no-save', '--no-audit', '--no-fund', ...specs];

/** Drops the packages a previous setup installed, so the same version is really installed again. */
export function removeInstalled() {
  rmSync(here('node_modules/@vitela'), { recursive: true, force: true });
}

/** A committed fixture as public/sample.pdf, the URL the pages open. */
export function copySample() {
  mkdirSync(here('public'), { recursive: true });
  copyFileSync(repo('tests/fixtures/compress/vector_only.pdf'), here('public/sample.pdf'));
}
