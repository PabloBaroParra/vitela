// Test support: an `Engine` whose every method fails unless a test supplies
// it, so each test spells out only the calls it expects. A copy of
// packages/vitela-viewer/src/test/fakeEngine.ts: the package ships no test
// helpers, and App.test.tsx is the only app test that needs one.

import type { Engine } from '@vitela/core';

const unused = (method: string) => () => Promise.reject(new Error(`${method}: not expected in this test`));

export function fakeEngine(overrides: Partial<Engine> = {}): Engine {
  return {
    open: unused('open'),
    renderPage: unused('renderPage'),
    close: async () => {},
    saveToBytes: unused('saveToBytes'),
    search: unused('search'),
    selectText: unused('selectText'),
    selectWord: unused('selectWord'),
    applyEdit: unused('applyEdit'),
    undo: unused('undo'),
    redo: unused('redo'),
    prepareImport: unused('prepareImport'),
    discardImport: unused('discardImport'),
    importPrepared: unused('importPrepared'),
    annotations: unused('annotations'),
    comments: unused('comments'),
    formFields: unused('formFields'),
    freeTextLayout: unused('freeTextLayout'),
    stampPlacement: unused('stampPlacement'),
    ...overrides,
  };
}
