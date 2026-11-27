// Vite plugin (`vite preview` only): routes requests the way the deployed
// site does (src/features/hosting/routes.ts). The tool routes public/_redirects
// rewrites get the shell, a file is itself, and any other path gets 404.html
// with a 404, where Vite's own preview would answer with the shell. So the
// e2e suite and the post-deploy smoke test (scripts/smoke.mjs, run against
// preview by e2e/smoke.spec.ts) see a stale /assets/<hash>.js get a 404, as
// on Cloudflare. Registered after hostHeaders(), so the headers are those of
// the path as requested, as Cloudflare applies them.

import { readFileSync, statSync } from 'node:fs';
import { resolve, sep } from 'node:path';
import type { Plugin } from 'vite';
import { parseRedirects, routeFor } from '../src/features/hosting/routes.ts';

export function hostRoutes(): Plugin {
  return {
    name: 'vitela-host-routes',
    configurePreviewServer(server) {
      const dist = resolve(server.config.root, server.config.build.outDir);
      const rules = parseRedirects(readFileSync(resolve(dist, '_redirects'), 'utf8'));
      const isFile = (path: string): boolean => {
        const file = resolve(dist, `.${path}`);
        if (file !== dist && !file.startsWith(dist + sep)) return false;
        try {
          return statSync(file).isFile();
        } catch {
          return false;
        }
      };
      server.middlewares.use((request, response, next) => {
        const url = new URL(request.url ?? '/', 'http://preview.invalid');
        let pathname: string;
        try {
          pathname = decodeURIComponent(url.pathname);
        } catch {
          pathname = url.pathname;
        }
        const route = routeFor(rules, pathname, isFile);
        switch (route.kind) {
          case 'file':
            // The shell is left to Vite's own serving of `/`; a rewritten
            // tool route becomes `/`, as Vite served it before.
            if (route.path === '/index.html') request.url = `/${url.search}`;
            else if (route.path !== pathname) request.url = `${route.path}${url.search}`;
            next();
            return;
          case 'redirect':
            response.statusCode = route.status;
            response.setHeader('Location', route.to);
            response.end();
            return;
          case 'notFound':
            response.statusCode = 404;
            response.setHeader('Content-Type', 'text/html; charset=utf-8');
            response.end(readFileSync(resolve(dist, '404.html'), 'utf8'));
            return;
        }
      });
    },
  };
}
