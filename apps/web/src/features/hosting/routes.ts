// Which file the deployed site answers a path with, the way Cloudflare Pages
// routes it: public/_redirects first (the tool routes are 200 rewrites to the
// shell), then a file at that path (`/` being index.html), else 404.html with
// a 404 status. `vite preview` routes through this (scripts/hostRoutes.ts):
// left to itself it would answer an unknown path with the shell, as a
// single-page app, which the deployed site never does. Pure, so Vitest pins
// it against the real file (routes.test.ts).
//
// Only what public/_redirects uses is read: exact source paths. Cloudflare's
// splats, placeholders and pretty URLs (`/about` serving about.html) are not
// emulated; the app ships none of them.

export interface RedirectRule {
  readonly from: string;
  readonly to: string;
  readonly status: number;
}

export type Route =
  | { readonly kind: 'file'; readonly path: string }
  | { readonly kind: 'redirect'; readonly to: string; readonly status: number }
  | { readonly kind: 'notFound'; readonly path: '/404.html' };

export function parseRedirects(text: string): RedirectRule[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('#'))
    .map((line) => {
      const [from = '', to = '', status] = line.split(/\s+/);
      return { from, to, status: status ? Number(status) : 302 };
    });
}

/** How the site answers `pathname`; `isFile` says whether the build has a file at a path. */
export function routeFor(rules: readonly RedirectRule[], pathname: string, isFile: (path: string) => boolean): Route {
  const rule = rules.find((candidate) => candidate.from === pathname);
  if (rule && rule.status !== 200) return { kind: 'redirect', to: rule.to, status: rule.status };
  const path = rule ? rule.to : pathname;
  // A directory is its index.html: `/` is the shell.
  const file = path.endsWith('/') ? `${path}index.html` : path;
  return isFile(file) ? { kind: 'file', path: file } : { kind: 'notFound', path: '/404.html' };
}
