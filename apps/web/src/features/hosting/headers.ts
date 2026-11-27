// public/_headers, read the way Cloudflare Pages reads it, so `vite preview`
// (scripts/hostHeaders.ts, the server every e2e spec runs against) answers
// with exactly the headers the deployed site sends — the Content-Security-
// Policy above all. The file is the one source of truth; this only parses it.
// Pure, so Vitest pins it against the real file (headers.test.ts).
//
// Cloudflare's rules (developers.cloudflare.com/pages/configuration/headers):
// a rule is an unindented URL pattern, its headers the indented `Name: value`
// lines under it; `*` (one per pattern) matches anything, `:name` one path
// segment. Every matching rule applies, in file order: a header set twice is
// joined with ", ", and `! Name` removes what earlier rules set.

/** One header line of a rule: a value to set, or a header to detach. */
export type HeaderEntry = { readonly name: string; readonly value: string } | { readonly detach: string };

export interface HeaderRule {
  readonly pattern: string;
  readonly entries: readonly HeaderEntry[];
}

export function parseHeaders(text: string): HeaderRule[] {
  const rules: Array<{ pattern: string; entries: HeaderEntry[] }> = [];
  for (const [index, raw] of text.split(/\r?\n/).entries()) {
    const line = raw.trim();
    if (line === '' || line.startsWith('#')) continue;
    if (!/^\s/.test(raw)) {
      rules.push({ pattern: line, entries: [] });
      continue;
    }
    const rule = rules.at(-1);
    if (!rule) throw new Error(`_headers line ${index + 1}: a header before any URL pattern`);
    if (line.startsWith('!')) {
      rule.entries.push({ detach: line.slice(1).trim() });
      continue;
    }
    const colon = line.indexOf(':');
    if (colon <= 0) throw new Error(`_headers line ${index + 1}: not a "Name: value" header`);
    rule.entries.push({ name: line.slice(0, colon).trim(), value: line.slice(colon + 1).trim() });
  }
  return rules;
}

/** Whether a rule's URL pattern matches `pathname`. */
export function matches(pattern: string, pathname: string): boolean {
  const source = pattern
    .split(/(\*|:[A-Za-z]\w*)/)
    .map((part) => {
      if (part === '*') return '.*';
      if (/^:[A-Za-z]\w*$/.test(part)) return '[^/]+';
      return part.replace(/[.+?^${}()|[\]\\]/g, '\\$&');
    })
    .join('');
  return new RegExp(`^${source}$`).test(pathname);
}

/** The headers `pathname` is served with, by their names as written. */
export function headersFor(rules: readonly HeaderRule[], pathname: string): Record<string, string> {
  const headers = new Map<string, { name: string; value: string }>();
  for (const rule of rules) {
    if (!matches(rule.pattern, pathname)) continue;
    for (const entry of rule.entries) {
      if ('detach' in entry) {
        headers.delete(entry.detach.toLowerCase());
        continue;
      }
      const key = entry.name.toLowerCase();
      const earlier = headers.get(key);
      headers.set(key, earlier ? { name: earlier.name, value: `${earlier.value}, ${entry.value}` } : { ...entry });
    }
  }
  return Object.fromEntries([...headers.values()].map(({ name, value }) => [name, value]));
}

/** A connect-style middleware that sets each response's `_headers` headers before the static files are served. */
export function headersMiddleware(rules: readonly HeaderRule[]) {
  return (
    request: { readonly url?: string },
    response: { setHeader(name: string, value: string): unknown },
    next: () => void,
  ): void => {
    const pathname = new URL(request.url ?? '/', 'http://preview.invalid').pathname;
    for (const [name, value] of Object.entries(headersFor(rules, pathname))) response.setHeader(name, value);
    next();
  };
}

/** A Content-Security-Policy, directive name → its sources, in order. */
export function parsePolicy(policy: string): Map<string, string[]> {
  const directives = new Map<string, string[]>();
  for (const directive of policy.split(';')) {
    const [name, ...sources] = directive.trim().split(/\s+/);
    if (name) directives.set(name.toLowerCase(), sources);
  }
  return directives;
}
