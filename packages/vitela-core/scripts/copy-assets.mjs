// Completes dist/ after tsc: the WASM modules the worker loads (next to it,
// at the same relative path as in src/) and the licence files npm packs.
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
];

for (const asset of ASSETS) {
  const from = pkg(`src/${asset}`);
  if (!existsSync(from)) {
    console.error(`error: missing src/${asset}; run \`npm run build:wasm\` first`);
    process.exit(1);
  }
  const to = pkg(`dist/${asset}`);
  mkdirSync(dirname(to), { recursive: true });
  copyFileSync(from, to);
}

for (const licence of ['LICENSE-MIT', 'LICENSE-APACHE']) {
  copyFileSync(repo(licence), pkg(licence));
}
