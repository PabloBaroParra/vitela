// Builds and serves the embed host fixture (index.html here) for
// e2e/embed.spec.ts. Its own build, into e2e/host/dist/ (git-ignored), so the
// fixture never ships in the app's dist/. Same worker/WASM settings as the app.
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';

export const HOST_PORT = 4174;

export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
  worker: { format: 'es' },
  build: {
    target: 'es2022',
    assetsInlineLimit: 0,
    outDir: 'dist',
    emptyOutDir: true,
  },
  preview: { port: HOST_PORT, host: '127.0.0.1' },
});
