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
// A release publishes exactly these tarballs (.github/workflows/npm-release.yml);
// `npm run setup:registry -- <version>` installs a published version instead.
import { mkdirSync, rmSync } from 'node:fs';
import { copySample, here, installArgs, npm, removeInstalled, repo } from './shared.mjs';

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

removeInstalled();
npm(installArgs(...tarballs), here('.'));
copySample();
console.log(`installed ${tarballs.join(', ')}; public/sample.pdf ready`);
