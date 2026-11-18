// Every tool route needs its own hosting rewrite, or the deployed site answers
// it with 404.html. This keeps public/_redirects and the registry in step.

import { describe, expect, it } from 'vitest';
import redirects from '../../../public/_redirects?raw';
import { TOOLS } from './tools';

const rules = redirects
  .split(/\r?\n/)
  .map((line) => line.trim())
  .filter((line) => line !== '' && !line.startsWith('#'))
  .map((line) => line.split(/\s+/));

describe('public/_redirects', () => {
  it('rewrites each tool path to the app shell with a 200', () => {
    for (const rule of rules) expect(rule.slice(1)).toEqual(['/', '200']);
  });

  it('lists exactly one rewrite per tool path and nothing else', () => {
    const sources = rules.map((rule) => rule[0]);
    expect([...sources].sort()).toEqual(TOOLS.map((tool) => tool.path).sort());
  });

  it('has no catch-all, so 404.html still answers unknown paths', () => {
    expect(rules.some((rule) => rule[0]?.includes('*'))).toBe(false);
  });
});
