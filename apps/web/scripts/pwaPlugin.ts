// Vite plugin (production builds only) that makes the app an installable,
// offline-capable PWA:
//
// - copies the brand icons from the repo (assets/brand, the macOS app icon,
//   the Play Store icon) into dist/icons/ and writes dist/manifest.webmanifest;
// - links both from index.html, with the theme colour;
// - once every file is written, lists dist/, versions it by content and
//   bundles src/features/pwa/sw.ts into dist/sw.js with that list and version.
//
// Hand-written rather than vite-plugin-pwa/Workbox: the worker is ~60 lines,
// needs one precache list and no runtime routes, and adds no dependency.

import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { build, type Plugin, type ResolvedConfig } from 'vite';
import { THEME_COLOR, webManifest } from '../src/features/pwa/manifest.ts';
import { cacheVersion, precacheUrls, type BuiltFile } from '../src/features/pwa/precache.ts';

const REPO = resolve(import.meta.dirname, '../../..');
const SW_SOURCE = resolve(import.meta.dirname, '../src/features/pwa/sw.ts');

/** dist/ path → the repo file it is copied from. */
const ICONS = {
  'icons/vitela.svg': 'assets/brand/vitela-app-mark.svg',
  'icons/vitela-512.png': 'apps/macos/Vitela/Assets.xcassets/AppIcon.appiconset/icon_512x512.png',
  'icons/vitela-maskable-512.png': 'docs/store/play/icon-512.png',
} as const;

const MANIFEST = 'manifest.webmanifest';

function sha256(bytes: string | Buffer): string {
  return createHash('sha256').update(bytes).digest('hex');
}

/** Every file under `dir`, as `/`-separated paths relative to it. */
function listFiles(dir: string): string[] {
  return readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => relative(dir, join(entry.parentPath, entry.name)).replace(/\\/g, '/'));
}

export function pwa(): Plugin {
  let config: ResolvedConfig;
  return {
    name: 'vitela-pwa',
    apply: 'build',
    configResolved(resolved) {
      config = resolved;
    },
    transformIndexHtml() {
      return [
        { tag: 'link', attrs: { rel: 'manifest', href: `/${MANIFEST}` }, injectTo: 'head' },
        { tag: 'meta', attrs: { name: 'theme-color', content: THEME_COLOR }, injectTo: 'head' },
        { tag: 'link', attrs: { rel: 'icon', href: '/icons/vitela.svg', type: 'image/svg+xml' }, injectTo: 'head' },
        { tag: 'link', attrs: { rel: 'apple-touch-icon', href: '/icons/vitela-maskable-512.png' }, injectTo: 'head' },
      ];
    },
    generateBundle() {
      for (const [fileName, source] of Object.entries(ICONS)) {
        this.emitFile({ type: 'asset', fileName, source: readFileSync(resolve(REPO, source)) });
      }
      const manifest = webManifest({
        svg: '/icons/vitela.svg',
        png512: '/icons/vitela-512.png',
        maskable512: '/icons/vitela-maskable-512.png',
      });
      this.emitFile({ type: 'asset', fileName: MANIFEST, source: `${JSON.stringify(manifest, null, 2)}\n` });
    },
    // After every file (the engine worker's WASM included) is on disk.
    writeBundle: {
      sequential: true,
      order: 'post',
      async handler() {
        const outDir = resolve(config.root, config.build.outDir);
        const files: BuiltFile[] = listFiles(outDir).map((path) => ({
          path,
          digest: sha256(readFileSync(join(outDir, path))),
        }));
        const urls = precacheUrls(files.map((file) => file.path));
        const version = cacheVersion(files, sha256);
        const output = await build({
          configFile: false,
          root: config.root,
          logLevel: 'warn',
          publicDir: false,
          define: {
            __VITELA_PRECACHE__: JSON.stringify(urls),
            __VITELA_CACHE_VERSION__: JSON.stringify(version),
          },
          build: {
            write: false,
            target: 'es2022',
            minify: true,
            lib: { entry: SW_SOURCE, formats: ['iife'], name: 'vitelaServiceWorker', fileName: () => 'sw.js' },
          },
        });
        const outputs = Array.isArray(output) ? output : [output];
        const chunk = outputs
          .flatMap((result) => ('output' in result ? result.output : []))
          .find((item) => item.type === 'chunk' && item.isEntry);
        if (!chunk || chunk.type !== 'chunk') throw new Error('vitela-pwa: the service worker bundle has no entry chunk');
        writeFileSync(join(outDir, 'sw.js'), chunk.code);
        config.logger.info(`vitela-pwa: sw.js precaches ${urls.length} files as cache version ${version}`);
      },
    },
  };
}
