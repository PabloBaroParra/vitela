// Which page a URL path shows. Each tool opens in its own tab, so the route
// is resolved once at load from `location.pathname`; there is no client-side
// navigation to keep in sync.

import { TOOLS, type Tool } from './tools';

export type Route = { readonly kind: 'home' } | { readonly kind: 'tool'; readonly tool: Tool } | { readonly kind: 'not-found' };

export function resolveRoute(pathname: string, tools: readonly Tool[] = TOOLS): Route {
  // Tolerate a trailing slash: `/edit/` is `/edit`.
  const path = pathname.length > 1 ? pathname.replace(/\/+$/, '') : pathname;
  if (path === '' || path === '/' || path === '/index.html') return { kind: 'home' };
  const tool = tools.find((candidate) => candidate.path === path);
  return tool ? { kind: 'tool', tool } : { kind: 'not-found' };
}
