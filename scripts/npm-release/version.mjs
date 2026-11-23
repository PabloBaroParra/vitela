// The versions an npm release of the web packages may carry: exact semver,
// prereleases allowed, build metadata not (npm drops it, so the tag would name
// a version the registry does not have).

const NUMBER = '(?:0|[1-9]\\d*)';
const IDENTIFIER = `(?:${NUMBER}|\\d*[A-Za-z-][0-9A-Za-z-]*)`;
const VERSION = new RegExp(`^(${NUMBER})\\.(${NUMBER})\\.(${NUMBER})(?:-(${IDENTIFIER}(?:\\.${IDENTIFIER})*))?$`);

/** `{ core, prerelease }` identifier lists, or null when `text` is not an exact version. */
export function parseVersion(text) {
  const match = typeof text === 'string' ? VERSION.exec(text) : null;
  return match && { core: match.slice(1, 4), prerelease: match[4]?.split('.') ?? [] };
}

function mustParse(text) {
  const version = parseVersion(text);
  if (!version) throw new Error(`'${text}' is not a semver version (build metadata is not allowed)`);
  return version;
}

function compareIdentifiers(a, b) {
  const numeric = [a, b].map((id) => /^\d+$/.test(id));
  if (numeric[0] !== numeric[1]) return numeric[0] ? -1 : 1; // numbers sort before words
  // No leading zeros, so the longer number is the larger one, at any size.
  if (numeric[0] && a.length !== b.length) return a.length - b.length;
  return a < b ? -1 : a > b ? 1 : 0;
}

/** Semver precedence: negative when `a` is lower than `b`, 0 when equal. Throws on anything else. */
export function compareVersions(a, b) {
  const [left, right] = [mustParse(a), mustParse(b)];
  for (const i of [0, 1, 2]) {
    const order = compareIdentifiers(left.core[i], right.core[i]);
    if (order !== 0) return order;
  }
  const [l, r] = [left.prerelease, right.prerelease];
  if (l.length === 0 || r.length === 0) return r.length - l.length; // a final version outranks its prereleases
  for (let i = 0; i < Math.min(l.length, r.length); i += 1) {
    const order = compareIdentifiers(l[i], r[i]);
    if (order !== 0) return order;
  }
  return l.length - r.length;
}

/** `npm install` without a version gets `latest`, so a prerelease never goes there. */
export function distTagFor(version) {
  return mustParse(version).prerelease.length > 0 ? 'next' : 'latest';
}
