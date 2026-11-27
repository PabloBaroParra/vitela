import { describe, expect, it, vi } from 'vitest';
import { headersFor, headersMiddleware, matches, parseHeaders, parsePolicy } from './headers';

describe('parseHeaders', () => {
  it('reads rules, their headers and detaches, skipping comments and blank lines', () => {
    const rules = parseHeaders(['# a comment', '/a/*', '  X-One: 1', '  ! X-Two', '', '/b', '\tX-Three: a: b'].join('\r\n'));
    expect(rules).toEqual([
      { pattern: '/a/*', entries: [{ name: 'X-One', value: '1' }, { detach: 'X-Two' }] },
      { pattern: '/b', entries: [{ name: 'X-Three', value: 'a: b' }] },
    ]);
  });

  it('refuses a header before any rule, or a header line with no name', () => {
    expect(() => parseHeaders('  X-One: 1')).toThrow(/line 1/);
    expect(() => parseHeaders('/a\n  no colon')).toThrow(/line 2/);
  });
});

describe('matches', () => {
  it('matches a splat anywhere, greedily, and a placeholder within one segment', () => {
    expect(matches('/*', '/')).toBe(true);
    expect(matches('/*', '/assets/index.js')).toBe(true);
    expect(matches('/assets/worker-*', '/assets/worker-BVc5DJRW.js')).toBe(true);
    expect(matches('/assets/worker-*', '/assets/index-BVc5DJRW.js')).toBe(false);
    expect(matches('/users/:id/profile', '/users/7/profile')).toBe(true);
    expect(matches('/users/:id/profile', '/users/7/8/profile')).toBe(false);
  });

  it('matches anything else literally, dots included', () => {
    expect(matches('/sw.js', '/sw.js')).toBe(true);
    expect(matches('/sw.js', '/swxjs')).toBe(false);
    expect(matches('/sw.js', '/sw.js/x')).toBe(false);
  });
});

describe('headersFor', () => {
  const rules = parseHeaders(
    [
      '/*',
      '  X-Robots-Tag: noindex',
      '  Policy: broad',
      '/static/*',
      '  X-Robots-Tag: nosnippet',
      '/static/special.js',
      '  ! Policy',
      '  Policy: narrow',
    ].join('\n'),
  );

  it('applies every matching rule, joining a header set twice with a comma', () => {
    expect(headersFor(rules, '/static/a.css')).toEqual({ 'X-Robots-Tag': 'noindex, nosnippet', Policy: 'broad' });
    expect(headersFor(rules, '/')).toEqual({ 'X-Robots-Tag': 'noindex', Policy: 'broad' });
  });

  it('lets a later rule detach an earlier header and set its own', () => {
    expect(headersFor(rules, '/static/special.js')).toEqual({ 'X-Robots-Tag': 'noindex, nosnippet', Policy: 'narrow' });
  });

  it('cannot detach what a later rule sets: rules apply in file order', () => {
    const reversed = parseHeaders(['/a', '  ! Policy', '/*', '  Policy: broad'].join('\n'));
    expect(headersFor(reversed, '/a')).toEqual({ Policy: 'broad' });
  });
});

describe('headersMiddleware', () => {
  it("sets the request path's headers, ignoring the query, then hands on", () => {
    const rules = parseHeaders('/a\n  X-One: 1\n/*\n  X-All: yes');
    const set = vi.fn();
    const next = vi.fn();
    headersMiddleware(rules)({ url: '/a?x=1' }, { setHeader: set }, next);
    expect(set.mock.calls).toEqual([
      ['X-One', '1'],
      ['X-All', 'yes'],
    ]);
    expect(next).toHaveBeenCalledTimes(1);
  });
});

describe('parsePolicy', () => {
  it('splits a policy into its directives and their sources', () => {
    const policy = parsePolicy("default-src 'none'; script-src 'self' blob: ;img-src data:;");
    expect([...policy]).toEqual([
      ['default-src', ["'none'"]],
      ['script-src', ["'self'", 'blob:']],
      ['img-src', ['data:']],
    ]);
  });
});
