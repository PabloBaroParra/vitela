// Removes dist/ so a build never ships files from an earlier one.
import { rmSync } from 'node:fs';

rmSync(new URL('../dist', import.meta.url), { recursive: true, force: true });
