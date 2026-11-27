// The web app manifest that makes Vitela installable. The build
// (scripts/pwaPlugin.ts) writes it to dist/manifest.webmanifest with the
// icons it copies from the repo's brand assets.

/** The editor's `--accent` (packages/vitela-viewer/src/styles/tokens.css). */
export const THEME_COLOR = '#1b2a57';
/** The editor's `--surface`: what the splash screen shows while the app loads. */
export const BACKGROUND_COLOR = '#ffffff';

export interface ManifestIcons {
  /** The brand mark, any size. */
  readonly svg: string;
  /** A 512 px rounded icon on a transparent ground. */
  readonly png512: string;
  /** A 512 px full-bleed icon with the mark inside the maskable safe zone. */
  readonly maskable512: string;
}

export function webManifest(icons: ManifestIcons) {
  return {
    name: 'Vitela',
    short_name: 'Vitela',
    description: 'Open, edit, sign and save PDFs in your browser. Your files never leave your device.',
    // The site root: Cloudflare Pages serves the app there, and every tool
    // route (/merge, /edit…) is inside the scope.
    id: '/',
    start_url: '/',
    scope: '/',
    display: 'standalone',
    theme_color: THEME_COLOR,
    background_color: BACKGROUND_COLOR,
    icons: [
      { src: icons.svg, sizes: 'any', type: 'image/svg+xml', purpose: 'any' },
      { src: icons.png512, sizes: '512x512', type: 'image/png', purpose: 'any' },
      { src: icons.maskable512, sizes: '512x512', type: 'image/png', purpose: 'maskable' },
    ],
  } as const;
}
