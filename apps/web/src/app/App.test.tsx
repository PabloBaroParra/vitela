import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
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

  // The page is all editor: a PDF dropped anywhere on it opens, not only
  // one dropped on the editor (the default of an embedded editor).
  it('opens a PDF dropped anywhere on the page, the site header included', async () => {
    const opened: Uint8Array[] = [];
    const engine = {
      ...fakeEngine({
        open: (bytes) => {
          opened.push(bytes);
          return new Promise(() => {});
        },
      }),
      ready: Promise.resolve(),
    } as unknown as EngineClient;
    const { container } = render(<App engine={engine} pathname="/" />);
    await act(async () => {});

    const pdf = new File([new Uint8Array([37, 80, 68, 70])], 'dropped.pdf', { type: 'application/pdf' });
    const header = container.querySelector('.site-header')!;
    for (const type of ['dragenter', 'dragover', 'drop']) {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, 'dataTransfer', { value: { types: ['Files'], files: [pdf], dropEffect: 'none' } });
      await act(async () => {
        header.dispatchEvent(event);
      });
    }
    await waitFor(() => expect(opened).toHaveLength(1));
  });

  describe('updates', () => {
    function fakeUpdates(ready: boolean) {
      const state = { ready, held: false, blocked: false };
      return {
        subscribe: () => () => {},
        state: () => state,
        hold: vi.fn(),
        apply: vi.fn(),
      };
    }
    const readyEngine = () => ({ ...fakeEngine(), ready: Promise.resolve() }) as unknown as EngineClient;

    it('shows no update banner while there is no new version', async () => {
      const { queryByRole } = render(<App engine={readyEngine()} pathname="/" updates={fakeUpdates(false)} />);
      await act(async () => {});
      expect(queryByRole('button', { name: 'Reload' })).toBeNull();
    });

    it('offers a new version and reloads into it on request', async () => {
      const updates = fakeUpdates(true);
      const { getByRole } = render(<App engine={readyEngine()} pathname="/" updates={updates} />);
      await act(async () => {});
      fireEvent.click(getByRole('button', { name: 'Reload' }));
      expect(updates.apply).toHaveBeenCalledTimes(1);
    });

    it('tells the updates whether the open document has unsaved edits', async () => {
      const updates = fakeUpdates(false);
      render(<App engine={readyEngine()} pathname="/" updates={updates} />);
      await act(async () => {});
      expect(updates.hold).toHaveBeenLastCalledWith(false);
    });
  });
});
