// Removes dist/ (`npm run clean`, and before `npm pack`): builds overwrite in
// place, so only a clean build is guaranteed to ship no file from an earlier one.
import { rmSync } from 'node:fs';

rmSync(new URL('../dist', import.meta.url), { recursive: true, force: true });
