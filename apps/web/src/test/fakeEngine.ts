// Test support: an `Engine` whose every method fails unless a test supplies
// it, so each test spells out only the calls it expects.

import type { Engine } from '../core/types';

const unused = (method: string) => () => Promise.reject(new Error(`${method}: not expected in this test`));

export function fakeEngine(overrides: Partial<Engine> = {}): Engine {
  return {
    open: unused('open'),
    renderPage: unused('renderPage'),
    close: async () => {},
    search: unused('search'),
    selectText: unused('selectText'),
    selectWord: unused('selectWord'),
    ...overrides,
  };
}
