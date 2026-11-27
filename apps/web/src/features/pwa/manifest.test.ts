import { describe, expect, it } from 'vitest';
import { BACKGROUND_COLOR, THEME_COLOR, webManifest } from './manifest';

describe('webManifest', () => {
  const manifest = webManifest({
    svg: '/icons/vitela.svg',
    png512: '/icons/vitela-512.png',
    maskable512: '/icons/vitela-maskable-512.png',
  });

  it('installs as Vitela, standalone, from the site root', () => {
    expect(manifest).toMatchObject({
      name: 'Vitela',
      short_name: 'Vitela',
      id: '/',
      start_url: '/',
      scope: '/',
      display: 'standalone',
    });
  });

  it("takes its colours from the editor's palette (--accent, --surface)", () => {
    expect(THEME_COLOR).toBe('#1b2a57');
    expect(BACKGROUND_COLOR).toBe('#ffffff');
    expect(manifest.theme_color).toBe(THEME_COLOR);
    expect(manifest.background_color).toBe(BACKGROUND_COLOR);
  });

  it('offers a scalable icon, a 512 px icon and a maskable one', () => {
    expect(manifest.icons).toEqual([
      { src: '/icons/vitela.svg', sizes: 'any', type: 'image/svg+xml', purpose: 'any' },
      { src: '/icons/vitela-512.png', sizes: '512x512', type: 'image/png', purpose: 'any' },
      { src: '/icons/vitela-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
    ]);
  });
});
