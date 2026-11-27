import { describe, expect, it } from 'vitest';
import redirects from '../../../public/_redirects?raw';
import { parseRedirects, routeFor } from './routes';
import { TOOLS } from '../tools/tools';

const files = new Set(['/index.html', '/404.html', '/sw.js', '/assets/index-abc.js']);
const isFile = (path: string) => files.has(path);

describe('parseRedirects', () => {
  it('reads each rule of public/_redirects as source, target and status', () => {
    const rules = parseRedirects(redirects);
    expect(rules.map((rule) => rule.from).sort()).toEqual(TOOLS.map((tool) => tool.path).sort());
    expect(rules.every((rule) => rule.to === '/' && rule.status === 200)).toBe(true);
  });

  it('defaults a rule without a status to a 302, as Cloudflare does', () => {
    expect(parseRedirects('# a comment\n\n/old /new\n')).toEqual([{ from: '/old', to: '/new', status: 302 }]);
  });
});

describe('routeFor (the deployed site, as Cloudflare Pages answers)', () => {
  const rules = parseRedirects(redirects);

  it('serves the shell for the home page and a rewritten tool route', () => {
    expect(routeFor(rules, '/', isFile)).toEqual({ kind: 'file', path: '/index.html' });
    expect(routeFor(rules, '/edit', isFile)).toEqual({ kind: 'file', path: '/index.html' });
  });

  it('serves a file that exists as itself', () => {
    expect(routeFor(rules, '/sw.js', isFile)).toEqual({ kind: 'file', path: '/sw.js' });
    expect(routeFor(rules, '/assets/index-abc.js', isFile)).toEqual({ kind: 'file', path: '/assets/index-abc.js' });
  });

  it('answers any other path with 404.html, never the shell', () => {
    expect(routeFor(rules, '/no-such-page', isFile)).toEqual({ kind: 'notFound', path: '/404.html' });
    // A stale hashed asset must not get HTML under the immutable cache rule.
    expect(routeFor(rules, '/assets/index-old.js', isFile)).toEqual({ kind: 'notFound', path: '/404.html' });
    expect(routeFor(rules, '/assets/', isFile)).toEqual({ kind: 'notFound', path: '/404.html' });
  });

  it('redirects where a rule says to', () => {
    expect(routeFor(parseRedirects('/old /new 301'), '/old', isFile)).toEqual({ kind: 'redirect', to: '/new', status: 301 });
  });
});
