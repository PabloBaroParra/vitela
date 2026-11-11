/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// The engine runs in a module worker (`new Worker(url, { type: 'module' })`),
// so workers are bundled as ES modules in production too.
export default defineConfig({
  plugins: [react()],
  worker: { format: 'es' },
  build: {
    target: 'es2022',
    // pdfium.wasm is ~5 MB; never inline WASM or the pdfium glue as data: URIs.
    assetsInlineLimit: 0,
  },
  preview: { port: 4173, host: '127.0.0.1' },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.{ts,tsx}'],
    restoreMocks: true,
    unstubGlobals: true,
  },
});
