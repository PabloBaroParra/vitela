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
    compressPresets: unused('compressPresets'),
    compressionRefusal: unused('compressionRefusal'),
    compressionInvalidatesSignatures: unused('compressionInvalidatesSignatures'),
    saveCompressedToBytes: unused('saveCompressedToBytes'),
    imageExportFormats: unused('imageExportFormats'),
    parsePageSelection: unused('parsePageSelection'),
    pageImageFileNames: unused('pageImageFileNames'),
    firstPageTooLargeToExport: unused('firstPageTooLargeToExport'),
    openImageExport: unused('openImageExport'),
    exportPageImage: unused('exportPageImage'),
    closeImageExport: unused('closeImageExport'),
    openPrintSnapshot: unused('openPrintSnapshot'),
    protectionRefusal: unused('protectionRefusal'),
    protectionInvalidatesSignatures: unused('protectionInvalidatesSignatures'),
    protectToBytes: unused('protectToBytes'),
    extractionRefusal: unused('extractionRefusal'),
    extractSourceIsSigned: unused('extractSourceIsSigned'),
    extractPages: unused('extractPages'),
    planSplit: unused('planSplit'),
    documentProperties: unused('documentProperties'),
    metadataRefusal: unused('metadataRefusal'),
    contentEditingRefusal: unused('contentEditingRefusal'),
    readPageContent: unused('readPageContent'),
    readTextBlocks: unused('readTextBlocks'),
    readParagraphBlocks: unused('readParagraphBlocks'),
    textRunStyle: unused('textRunStyle'),
    pageFontFamilies: unused('pageFontFamilies'),
    contentBaseGeneration: unused('contentBaseGeneration'),
    watermarkDefaults: unused('watermarkDefaults'),
    watermarkRefusal: unused('watermarkRefusal'),
    ...overrides,
  };
}
