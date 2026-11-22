/// <reference types="node" />
// @vitest-environment node
// The stylesheet stays inside the editor's root (`.vitela`): a host page that
// embeds the editor keeps its own `:root`, `body` and element styles, and the
// editor sizes itself from its container, not from the viewport.

import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = fileURLToPath(new URL('.', import.meta.url));
const sheets = readdirSync(dir)
  .filter((name) => name.endsWith('.css'))
  .map((name) => ({ name, css: readFileSync(`${dir}/${name}`, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '') }));

/** The selector list of every style rule (not at-rules, not keyframe steps). */
function rulePreludes(css: string): string[] {
  const preludes: string[] = [];
  const open: string[] = [];
  let buffer = '';
  for (const char of css) {
    if (char === '{') {
      const prelude = buffer.trim();
      const inKeyframes = open.some((block) => block.startsWith('@keyframes'));
      if (!prelude.startsWith('@') && !inKeyframes) preludes.push(prelude);
      open.push(prelude);
      buffer = '';
    } else if (char === '}') {
      open.pop();
      buffer = '';
    } else if (char === ';') {
      buffer = '';
    } else {
      buffer += char;
    }
  }
  return preludes;
}

/** The first compound selector: up to the first combinator outside parentheses. */
function firstCompound(selector: string): string {
  let depth = 0;
  for (let i = 0; i < selector.length; i++) {
    const char = selector[i]!;
    if (char === '(') depth++;
    else if (char === ')') depth--;
    else if (depth === 0 && /[\s>+~]/.test(char)) return selector.slice(0, i);
  }
  return selector;
}

/** The declarations of the first rule whose prelude matches `prelude`. */
function blockOf(css: string, prelude: RegExp): string | undefined {
  const match = prelude.exec(css);
  if (!match) return undefined;
  const start = match.index + match[0].length;
  return css.slice(start, css.indexOf('}', start));
}

describe('editor stylesheet scope', () => {
  it('starts every selector at a class, so nothing matches host elements by type, :root or body', () => {
    const unscoped = sheets.flatMap(({ name, css }) =>
      rulePreludes(css)
        .flatMap((prelude) => prelude.split(','))
        .map((selector) => selector.trim())
        .filter((selector) => !firstCompound(selector).includes('.'))
        .map((selector) => `${name}: ${selector}`),
    );
    expect(unscoped).toEqual([]);
  });

  it('gives element resets no more weight than they had unscoped', () => {
    // `.vitela button` would outrank `.button--primary`; `:where()` adds nothing.
    const weighted = sheets.flatMap(({ name, css }) =>
      rulePreludes(css)
        .flatMap((prelude) => prelude.split(','))
        .map((selector) => selector.trim())
        .filter((selector) => /^\.vitela\s/.test(selector))
        .map((selector) => `${name}: ${selector}`),
    );
    expect(weighted).toEqual([]);
  });

  it('defines the tokens on .vitela, light and dark', () => {
    const tokens = sheets.find(({ name }) => name === 'tokens.css')!.css;
    expect(tokens).not.toMatch(/:root/);
    expect(blockOf(tokens, /(^|\n)\.vitela\s*\{/)).toMatch(/--surface:/);
    const dark = tokens.slice(tokens.indexOf('@media (prefers-color-scheme: dark)'));
    expect(blockOf(dark, /\.vitela\s*\{/)).toMatch(/--surface:/);
  });

  it('sizes nothing from the viewport height and pins nothing to the viewport', () => {
    for (const { name, css } of sheets) {
      expect({ name, vh: /\b100d?vh\b/.test(css) }).toEqual({ name, vh: false });
      expect({ name, fixed: /position:\s*fixed/.test(css) }).toEqual({ name, fixed: false });
    }
  });

  it('sizes text from the editor root, not from the host page root (no rem)', () => {
    for (const { name, css } of sheets) {
      expect({ name, rem: /\d(\.\d+)?rem\b/.test(css) }).toEqual({ name, rem: false });
    }
  });
});
