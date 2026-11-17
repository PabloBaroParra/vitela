// Completes dist/ after tsc: the WASM modules the worker loads (next to it,
// at the same relative path as in src/), their digests, and the licence files
// npm packs. Overwrites in place: dist/ is never wiped here, so a dev server
// reading it keeps working while this runs (see watch.mjs).
import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const pkg = (path) => fileURLToPath(new URL(`../${path}`, import.meta.url));
const repo = (path) => fileURLToPath(new URL(`../../../${path}`, import.meta.url));

const ASSETS = [
  'generated/engine/pdf_wasm.js',
  'generated/engine/pdf_wasm.d.ts',
  'generated/engine/pdf_wasm_bg.wasm',
  'generated/pdfium/pdfium.js',
  'generated/pdfium/pdfium.wasm',
  'generated/pdfium/VERSION',
  // scripts/write-integrity.mjs, from the same source files as above.
  'generated/integrity.js',
];

for (const asset of ASSETS) {
  const from = pkg(`src/${asset}`);
  if (!existsSync(from)) {
    console.error(`error: missing src/${asset}; run \`npm run build:wasm\` (and write-integrity.mjs) first`);
    process.exit(1);
  }
  const to = pkg(`dist/${asset}`);
  mkdirSync(dirname(to), { recursive: true });
  copyFileSync(from, to);
}

for (const licence of ['LICENSE-MIT', 'LICENSE-APACHE']) {
  copyFileSync(repo(licence), pkg(licence));
}
