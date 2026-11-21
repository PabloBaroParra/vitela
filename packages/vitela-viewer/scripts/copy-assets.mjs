// Completes dist/ after tsc: the stylesheets (tsc only emits the TypeScript)
// and the licence files npm packs. Overwrites in place: dist/ is never wiped
// here, so a dev server reading it keeps working while this runs (see watch.mjs).
import { copyFileSync, mkdirSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const pkg = (path) => fileURLToPath(new URL(`../${path}`, import.meta.url));
const repo = (path) => fileURLToPath(new URL(`../../../${path}`, import.meta.url));

mkdirSync(pkg('dist/styles'), { recursive: true });
for (const sheet of readdirSync(pkg('src/styles')).filter((name) => name.endsWith('.css'))) {
  copyFileSync(pkg(`src/styles/${sheet}`), pkg(`dist/styles/${sheet}`));
}

for (const licence of ['LICENSE-MIT', 'LICENSE-APACHE']) {
  copyFileSync(repo(licence), pkg(licence));
}
