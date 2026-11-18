import { act, cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { EngineClient } from '@vitela/core';
import { fakeEngine } from '../test/fakeEngine';
import { App } from './App';

afterEach(cleanup);

function loadingEngine() {
  let finish!: () => void;
  const ready = new Promise<void>((resolve) => (finish = resolve));
  const engine = { ...fakeEngine(), ready } as unknown as EngineClient;
  return { engine, finish };
}

describe('App', () => {
  // A remount replays the landing's entrance animation, so the page jumps
  // twice on every load.
  it('keeps the home page mounted while the engine finishes loading', async () => {
    const { engine, finish } = loadingEngine();
    const { container, getByLabelText } = render(<App engine={engine} pathname="/" />);
    const hero = container.querySelector('.hero');
    expect(hero).not.toBeNull();

    await act(async () => finish());

    expect(getByLabelText('Open PDF')).toBeTruthy();
    expect(container.querySelector('.hero')).toBe(hero);
  });
});
