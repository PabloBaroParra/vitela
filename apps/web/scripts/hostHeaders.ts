// Vite plugin (`vite preview` only): serves dist/ with the response headers
// public/_headers gives the deployed site (copied into dist/ by the build), so
// the e2e suite runs under the very Content-Security-Policy that ships. The
// dev server is left alone: its hot reload needs inline scripts.

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Plugin } from 'vite';
import { headersMiddleware, parseHeaders } from '../src/features/hosting/headers.ts';

export function hostHeaders(): Plugin {
  return {
    name: 'vitela-host-headers',
    configurePreviewServer(server) {
      const file = resolve(server.config.root, server.config.build.outDir, '_headers');
      server.middlewares.use(headersMiddleware(parseHeaders(readFileSync(file, 'utf8'))));
    },
  };
}
