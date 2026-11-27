import { describe, expect, it } from 'vitest';
import { cacheName, precacheKey, staleCaches } from './swRules';

const ORIGIN = 'https://vitela.example';
const precached = new Set(['/', '/assets/index-DkBMLp3E.js', '/assets/pdfium-8k7OGqr9.wasm', '/manifest.webmanifest']);

function request(path: string, mode: RequestMode = 'cors', method = 'GET') {
  return { method, mode, url: new URL(path, ORIGIN).href };
}

describe('cacheName / staleCaches', () => {
  it('names one cache per build version', () => {
    expect(cacheName('0123456789abcdef')).toBe('vitela-0123456789abcdef');
  });

  it("drops every older build's cache and leaves caches that are not ours alone", () => {
    expect(staleCaches(['vitela-old1', 'vitela-new2', 'someone-else', 'vitela-old3'], 'vitela-new2')).toEqual([
      'vitela-old1',
      'vitela-old3',
    ]);
  });
});

describe('precacheKey', () => {
  it('answers a built asset from the cache', () => {
    expect(precacheKey(request('/assets/pdfium-8k7OGqr9.wasm'), ORIGIN, precached)).toBe('/assets/pdfium-8k7OGqr9.wasm');
  });

  it('answers every page navigation with the app shell, tool routes included', () => {
    expect(precacheKey(request('/', 'navigate'), ORIGIN, precached)).toBe('/');
    expect(precacheKey(request('/organize', 'navigate'), ORIGIN, precached)).toBe('/');
    expect(precacheKey(request('/merge?x=1', 'navigate'), ORIGIN, precached)).toBe('/');
  });

  it('leaves requests to other origins to the network', () => {
    expect(precacheKey(request('https://cdn.example/assets/index-DkBMLp3E.js'), ORIGIN, precached)).toBeNull();
    expect(precacheKey(request('https://cdn.example/', 'navigate'), ORIGIN, precached)).toBeNull();
  });

  it('leaves anything that is not a GET to the network', () => {
    expect(precacheKey(request('/assets/index-DkBMLp3E.js', 'cors', 'POST'), ORIGIN, precached)).toBeNull();
    expect(precacheKey(request('/', 'navigate', 'POST'), ORIGIN, precached)).toBeNull();
  });

  it('never caches what is not in the build, such as a PDF', () => {
    expect(precacheKey(request('/document.pdf'), ORIGIN, precached)).toBeNull();
    expect(precacheKey(request('/assets/unknown.js'), ORIGIN, precached)).toBeNull();
  });

  it('does not answer a built asset asked for with a query string', () => {
    expect(precacheKey(request('/assets/index-DkBMLp3E.js?v=2'), ORIGIN, precached)).toBeNull();
  });
});
