import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// A consumer's config. The editor needs nothing here: no optimizeDeps.exclude,
// no worker or build options (checked with Vite 8.3.8, dev and build; see
// packages/vitela-viewer/README.md). Only the React plugin for this page's JSX
// and the two HTML entries.
export default defineConfig({
  plugins: [react()],
  build: {
    rollupOptions: {
      input: {
        react: fileURLToPath(new URL('index.html', import.meta.url)),
        vanilla: fileURLToPath(new URL('vanilla.html', import.meta.url)),
      },
    },
  },
  server: { host: '127.0.0.1' },
  preview: { host: '127.0.0.1' },
});
