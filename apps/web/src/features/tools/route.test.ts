import { describe, expect, it } from 'vitest';
import { resolveRoute } from './route';
import { TOOLS } from './tools';

describe('resolveRoute', () => {
  it('resolves the root to the home page', () => {
    expect(resolveRoute('/')).toEqual({ kind: 'home' });
    expect(resolveRoute('')).toEqual({ kind: 'home' });
    expect(resolveRoute('/index.html')).toEqual({ kind: 'home' });
  });

  it('resolves every tool path to its tool', () => {
    for (const tool of TOOLS) {
      expect(resolveRoute(tool.path)).toEqual({ kind: 'tool', tool });
    }
  });

  it('tolerates a trailing slash', () => {
    const route = resolveRoute('/edit/');
    expect(route.kind === 'tool' && route.tool.id).toBe('edit');
    expect(resolveRoute('/view//')).toMatchObject({ kind: 'tool', tool: { id: 'view' } });
  });

  it('reports anything else as not found', () => {
    expect(resolveRoute('/nope')).toEqual({ kind: 'not-found' });
    expect(resolveRoute('/edit/more')).toEqual({ kind: 'not-found' });
    expect(resolveRoute('/EDIT')).toEqual({ kind: 'not-found' });
  });

  it('gives every tool a unique, rooted path', () => {
    const paths = TOOLS.map((tool) => tool.path);
    expect(new Set(paths).size).toBe(paths.length);
    for (const path of paths) expect(path).toMatch(/^\/[a-z0-9-]+$/);
  });
});
