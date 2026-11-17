// Decides whether a push to main changes what the web serves, for
// .github/workflows/web-deploy.yml.
//
// The web is its own files plus the WASM engine, and the engine is built from
// core/pdf-wasm and every local crate it depends on. That set is read from
// `cargo metadata` on each run, so a crate pdf-wasm starts to use is covered
// without editing a list.
//
// Usage (from the repository root):
//   git diff --name-only BEFORE AFTER | node apps/web/scripts/deploy-needed.mjs
// Prints `true` or `false`.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

// Files of the web itself, and repository-wide build inputs of the engine.
const WEB_PREFIXES = ['apps/web/', 'packages/', '.cargo/', '.github/workflows/web-deploy.yml'];
const BUILD_FILES = ['package.json', 'package-lock.json', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain', 'rust-toolchain.toml'];

const slash = (path) => path.replaceAll('\\', '/');

/** Repository-relative directories (ending in `/`) of the local crates the engine is built from. */
export function engineCrateDirs(metadata, root) {
  const packages = new Map(metadata.packages.map((p) => [p.id, p]));
  const nodes = new Map(metadata.resolve.nodes.map((n) => [n.id, n]));
  const start = metadata.packages.filter((p) => p.name === 'pdf-wasm').map((p) => p.id);
  if (start.length === 0) throw new Error('pdf-wasm is not in the cargo workspace');

  const seen = new Set();
  const todo = [...start];
  while (todo.length > 0) {
    const id = todo.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      // dev-dependencies only build tests, never the shipped engine.
      if (dep.dep_kinds.some((k) => k.kind !== 'dev')) todo.push(dep.pkg);
    }
  }

  const prefix = slash(root).replace(/\/?$/, '/');
  return [...seen]
    .map((id) => packages.get(id))
    .filter((p) => p.source === null)
    .map((p) => slash(p.manifest_path).replace(/Cargo\.toml$/, ''))
    .map((dir) => (dir.startsWith(prefix) ? dir.slice(prefix.length) : dir));
}

/** Whether any changed path is part of what the web serves. */
export function deployNeeded(changed, crateDirs) {
  const prefixes = [...WEB_PREFIXES, ...crateDirs];
  return changed.map(slash).some((file) => BUILD_FILES.includes(file) || prefixes.some((p) => file.startsWith(p)));
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const changed = readFileSync(0, 'utf8').split('\n').map((l) => l.trim()).filter(Boolean);
  const metadata = JSON.parse(
    execFileSync('cargo', ['metadata', '--format-version', '1', '--locked', '--filter-platform', 'wasm32-unknown-unknown'], {
      encoding: 'utf8',
      maxBuffer: 256 * 1024 * 1024,
    }),
  );
  const dirs = engineCrateDirs(metadata, metadata.workspace_root);
  console.error(`engine crates: ${dirs.join(' ')}`);
  console.log(String(deployNeeded(changed, dirs)));
}
