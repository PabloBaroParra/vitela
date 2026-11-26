import { describe, expect, it } from 'vitest';
import { handleRequest } from './handler.js';
import type { Engine } from '../core/types.js';

const page = { width: 2, height: 1, rgba: new Uint8ClampedArray(8) };
const info = {
  pageCount: 1,
  pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }],
  pageIds: [0],
  textExtractionAllowed: true,
  documentAssemblyAllowed: true,
  blocks: [{ source: { kind: 'base' as const }, part: null, start: 0, count: 1 }],
  importRefusal: null,
  annotationEditingAllowed: true,
  formFillAllowed: true,
};
const edited = { info, canUndo: true, canRedo: false, annotations: [], addedAnnotationId: null, formFields: [] };
const opened = { handle: 5, info };
const compressed = {
  bytes: new Uint8Array([37, 80, 68, 70]),
  beforeBytes: 10,
  afterBytes: 4,
  savedBytes: 6,
  reduced: true,
  refusals: [],
};

function fakeEngine(overrides: Partial<Engine> = {}): Engine {
  return {
    open: async () => opened,
    renderPage: async () => page,
    close: async () => {},
    search: async () => [],
    selectText: async () => ({ text: '', rects: new Float32Array(0) }),
    selectWord: async () => ({ text: '', rects: new Float32Array(0) }),
    saveToBytes: async () => new Uint8Array(0),
    applyEdit: async () => edited,
    undo: async () => edited,
    redo: async () => edited,
    prepareImport: async () => ({ id: 1, pageCount: 1, warnings: [] }),
    discardImport: async () => {},
    importPrepared: async () => ({ ...edited, sourceIds: [1] }),
    annotations: async () => [],
    comments: async () => [],
    formFields: async () => [],
    freeTextLayout: async () => ({ fontSizePt: 12, lines: [], overflow: false }),
    stampPlacement: async () => ({ left: 0, top: 0, width: 144, height: 144 }),
    compressPresets: async () => ['lossless', 'balanced', 'small'],
    compressionRefusal: async () => null,
    compressionInvalidatesSignatures: async () => false,
    saveCompressedToBytes: async () => compressed,
    imageExportFormats: async () => ['png', 'jpeg'],
    parsePageSelection: async () => [0, 2],
    pageImageFileNames: async () => ['a-1.png'],
    firstPageTooLargeToExport: async () => null,
    openImageExport: async () => ({ handle: 9, pageCount: 3 }),
    exportPageImage: async () => new Uint8Array([0x89, 0x50]),
    closeImageExport: async () => true,
    protectionRefusal: async () => null,
    protectionInvalidatesSignatures: async () => false,
    protectToBytes: async () => new Uint8Array(0),
    ...overrides,
  };
}

describe('handleRequest', () => {
  it('answers open with the handle and document info and transfers nothing', async () => {
    const reply = await handleRequest(fakeEngine(), {
      id: 7,
      request: { kind: 'open', bytes: new Uint8Array([1]) },
    });
    expect(reply).toEqual({ message: { type: 'result', id: 7, value: opened }, transfer: [] });
  });

  it('answers renderPage and transfers the pixel buffer instead of copying it', async () => {
    const calls: [number, number, number][] = [];
    const engine = fakeEngine({
      renderPage: async (document, index, dpi) => {
        calls.push([document, index, dpi]);
        return page;
      },
    });
    const reply = await handleRequest(engine, {
      id: 8,
      request: { kind: 'renderPage', document: 5, pageIndex: 2, dpi: 96 },
    });
    expect(calls).toEqual([[5, 2, 96]]);
    expect(reply.message).toEqual({ type: 'result', id: 8, value: page });
    expect(reply.transfer).toEqual([page.rgba.buffer]);
  });

  it('turns an engine failure into an error reply for that request', async () => {
    const engine = fakeEngine({
      open: async () => {
        throw new Error('open: PdfiumLibraryInternalError(FormatError)');
      },
    });
    const reply = await handleRequest(engine, {
      id: 9,
      request: { kind: 'open', bytes: new Uint8Array() },
    });
    expect(reply).toEqual({
      message: { type: 'error', id: 9, name: 'Error', message: 'open: PdfiumLibraryInternalError(FormatError)' },
      transfer: [],
    });
  });

  it('passes the password of an open request to the engine', async () => {
    const seen: (string | undefined)[] = [];
    const engine = fakeEngine({
      open: async (_bytes, password) => {
        seen.push(password);
        return opened;
      },
    });
    await handleRequest(engine, { id: 1, request: { kind: 'open', bytes: new Uint8Array(), password: 'secret' } });
    await handleRequest(engine, { id: 2, request: { kind: 'open', bytes: new Uint8Array() } });
    expect(seen).toEqual(['secret', undefined]);
  });

  it('forwards the name of a typed engine error, not only its message', async () => {
    const failure = new Error('the password does not open this document');
    failure.name = 'WrongPassword';
    const engine = fakeEngine({
      open: async () => {
        throw failure;
      },
    });
    const reply = await handleRequest(engine, { id: 3, request: { kind: 'open', bytes: new Uint8Array() } });
    expect(reply.message).toEqual({
      type: 'error',
      id: 3,
      name: 'WrongPassword',
      message: 'the password does not open this document',
    });
  });

  it('names a thrown non-Error value generically', async () => {
    const engine = fakeEngine({
      renderPage: async () => {
        throw 'pdfium aborted';
      },
    });
    const reply = await handleRequest(engine, { id: 4, request: { kind: 'renderPage', document: 5, pageIndex: 0, dpi: 72 } });
    expect(reply.message).toEqual({ type: 'error', id: 4, name: 'Error', message: 'pdfium aborted' });
  });

  it('closes the document a close request names', async () => {
    const closed: number[] = [];
    const engine = fakeEngine({
      close: async (document) => {
        closed.push(document);
      },
    });
    const reply = await handleRequest(engine, { id: 6, request: { kind: 'close', document: 5 } });
    expect(closed).toEqual([5]);
    expect(reply).toEqual({ message: { type: 'result', id: 6, value: undefined }, transfer: [] });
  });

  it('answers the text requests with what the engine found for that document and page', async () => {
    const hits = [{ pageIndex: 1, rects: new Float32Array([1, 2, 3, 4]) }];
    const selection = { text: 'beta', rects: new Float32Array([5, 6, 7, 8]) };
    const calls: unknown[][] = [];
    const engine = fakeEngine({
      search: async (...args) => (calls.push(['search', ...args]), hits),
      selectText: async (...args) => (calls.push(['selectText', ...args]), selection),
      selectWord: async (...args) => (calls.push(['selectWord', ...args]), selection),
    });
    const a = { x: 1, y: 2 };
    const b = { x: 3, y: 4 };
    expect((await handleRequest(engine, { id: 1, request: { kind: 'search', document: 5, query: 'beta' } })).message).toEqual({
      type: 'result',
      id: 1,
      value: hits,
    });
    const selected = await handleRequest(engine, {
      id: 2,
      request: { kind: 'selectText', document: 5, pageIndex: 1, anchor: a, focus: b },
    });
    expect(selected.message).toEqual({ type: 'result', id: 2, value: selection });
    await handleRequest(engine, { id: 3, request: { kind: 'selectWord', document: 5, pageIndex: 2, point: a } });
    expect(calls).toEqual([
      ['search', 5, 'beta'],
      ['selectText', 5, 1, a, b],
      ['selectWord', 5, 2, a],
    ]);
  });

  it('forwards TextExtractionDenied by name, like the password failures', async () => {
    const failure = new Error('text extraction is not permitted');
    failure.name = 'TextExtractionDenied';
    const engine = fakeEngine({
      search: async () => {
        throw failure;
      },
    });
    const reply = await handleRequest(engine, { id: 7, request: { kind: 'search', document: 5, query: 'x' } });
    expect(reply.message).toMatchObject({ type: 'error', id: 7, name: 'TextExtractionDenied' });
  });

  it('answers save with the bytes, transferred instead of copied', async () => {
    const bytes = new Uint8Array([37, 80, 68, 70]);
    const calls: [number, boolean][] = [];
    const engine = fakeEngine({
      saveToBytes: async (document, invalidate) => {
        calls.push([document, invalidate]);
        return bytes;
      },
    });
    const reply = await handleRequest(engine, {
      id: 4,
      request: { kind: 'save', document: 5, invalidateSignatures: true },
    });
    expect(calls).toEqual([[5, true]]);
    expect(reply).toEqual({ message: { type: 'result', id: 4, value: bytes }, transfer: [bytes.buffer] });
  });

  it('answers the compress questions for the document they name, copied', async () => {
    const calls: unknown[][] = [];
    const engine = fakeEngine({
      compressionRefusal: async (document) => (calls.push(['refusal', document]), { kind: 'other', message: 'no' }),
      compressionInvalidatesSignatures: async (document) => (calls.push(['signed', document]), true),
    });
    expect(await handleRequest(engine, { id: 1, request: { kind: 'compressPresets' } })).toEqual({
      message: { type: 'result', id: 1, value: ['lossless', 'balanced', 'small'] },
      transfer: [],
    });
    expect(await handleRequest(engine, { id: 2, request: { kind: 'compressionRefusal', document: 5 } })).toEqual({
      message: { type: 'result', id: 2, value: { kind: 'other', message: 'no' } },
      transfer: [],
    });
    expect(
      await handleRequest(engine, { id: 3, request: { kind: 'compressionInvalidatesSignatures', document: 5 } }),
    ).toEqual({ message: { type: 'result', id: 3, value: true }, transfer: [] });
    expect(calls).toEqual([
      ['refusal', 5],
      ['signed', 5],
    ]);
  });

  it('answers a compressed save with its report, the PDF transferred instead of copied', async () => {
    const calls: unknown[][] = [];
    const engine = fakeEngine({
      saveCompressedToBytes: async (...args) => (calls.push(args), compressed),
    });
    const reply = await handleRequest(engine, {
      id: 6,
      request: { kind: 'saveCompressed', document: 5, preset: 'balanced', invalidateSignatures: true },
    });
    expect(calls).toEqual([[5, 'balanced', true]]);
    expect(reply).toEqual({
      message: { type: 'result', id: 6, value: compressed },
      transfer: [compressed.bytes.buffer],
    });
  });

  it('answers the protect questions for the document they name, copied', async () => {
    const calls: unknown[][] = [];
    const engine = fakeEngine({
      protectionRefusal: async (document) => (calls.push(['refusal', document]), 'no'),
      protectionInvalidatesSignatures: async (document) => (calls.push(['signed', document]), true),
    });
    expect(await handleRequest(engine, { id: 1, request: { kind: 'protectionRefusal', document: 5 } })).toEqual({
      message: { type: 'result', id: 1, value: 'no' },
      transfer: [],
    });
    expect(
      await handleRequest(engine, { id: 2, request: { kind: 'protectionInvalidatesSignatures', document: 5 } }),
    ).toEqual({ message: { type: 'result', id: 2, value: true }, transfer: [] });
    expect(calls).toEqual([
      ['refusal', 5],
      ['signed', 5],
    ]);
  });

  it('answers a protect request with the protected PDF, transferred instead of copied', async () => {
    const calls: unknown[][] = [];
    const bytes = new Uint8Array([37, 80, 68, 70]);
    const engine = fakeEngine({ protectToBytes: async (...args) => (calls.push(args), bytes) });
    const reply = await handleRequest(engine, {
      id: 7,
      request: { kind: 'protect', document: 5, openPassword: 'o', permissionsPassword: 'p', invalidateSignatures: true },
    });
    expect(calls).toEqual([[5, { openPassword: 'o', permissionsPassword: 'p' }, true]]);
    expect(reply).toEqual({ message: { type: 'result', id: 7, value: bytes }, transfer: [bytes.buffer] });
  });

  it('forwards InvalidPasswords by name, and its reply carries no password', async () => {
    const engine = fakeEngine({
      protectToBytes: async () => {
        throw Object.assign(new Error('the open and permissions passwords must be different'), { name: 'InvalidPasswords' });
      },
    });
    const reply = await handleRequest(engine, {
      id: 8,
      request: { kind: 'protect', document: 5, openPassword: 'secret-1', permissionsPassword: 'secret-1', invalidateSignatures: false },
    });
    expect(reply.message).toEqual({
      type: 'error',
      id: 8,
      name: 'InvalidPasswords',
      message: 'the open and permissions passwords must be different',
    });
    expect(JSON.stringify(reply)).not.toContain('secret-1');
  });

  it('answers the image-export questions with what the engine says, copied', async () => {
    const calls: unknown[][] = [];
    const engine = fakeEngine({
      parsePageSelection: async (...args) => (calls.push(['parse', ...args]), [0, 2]),
      pageImageFileNames: async (...args) => (calls.push(['names', ...args]), ['r-1.png', 'r-3.png']),
      firstPageTooLargeToExport: async (...args) => (calls.push(['ceiling', ...args]), 2),
      openImageExport: async (...args) => (calls.push(['open', ...args]), { handle: 9, pageCount: 3 }),
      closeImageExport: async (...args) => (calls.push(['close', ...args]), true),
    });
    const value = async (id: number, request: Parameters<typeof handleRequest>[1]['request']) =>
      (await handleRequest(engine, { id, request })).message;
    expect(await value(1, { kind: 'imageExportFormats' })).toEqual({ type: 'result', id: 1, value: ['png', 'jpeg'] });
    expect(await value(2, { kind: 'parsePageSelection', input: '1,3', totalPages: 3 })).toEqual({
      type: 'result',
      id: 2,
      value: [0, 2],
    });
    expect(
      await value(3, { kind: 'pageImageFileNames', documentName: 'r.pdf', pages: [0, 2], totalPages: 3, format: 'png' }),
    ).toEqual({ type: 'result', id: 3, value: ['r-1.png', 'r-3.png'] });
    expect(await value(4, { kind: 'firstPageTooLargeToExport', document: 5, pages: [0, 2], dpi: 600 })).toEqual({
      type: 'result',
      id: 4,
      value: 2,
    });
    expect(await value(5, { kind: 'openImageExport', document: 5 })).toEqual({
      type: 'result',
      id: 5,
      value: { handle: 9, pageCount: 3 },
    });
    expect(await value(6, { kind: 'closeImageExport', export: 9 })).toEqual({ type: 'result', id: 6, value: true });
    expect(calls).toEqual([
      ['parse', '1,3', 3],
      ['names', 'r.pdf', [0, 2], 3, 'png'],
      ['ceiling', 5, [0, 2], 600],
      ['open', 5],
      ['close', 9],
    ]);
  });

  it('answers an exported page with its bytes transferred instead of copied', async () => {
    const image = new Uint8Array([0x89, 0x50, 0x4e, 0x47]);
    const calls: unknown[][] = [];
    const engine = fakeEngine({ exportPageImage: async (...args) => (calls.push(args), image) });
    const reply = await handleRequest(engine, {
      id: 7,
      request: { kind: 'exportPageImage', export: 9, pageIndex: 1, dpi: 150, format: 'jpeg' },
    });
    expect(calls).toEqual([[9, 1, 150, 'jpeg']]);
    expect(reply).toEqual({ message: { type: 'result', id: 7, value: image }, transfer: [image.buffer] });
  });

  it("forwards InvalidPageSelection by name with the core's sentence", async () => {
    const failure = new Error('Page 9 does not exist');
    failure.name = 'InvalidPageSelection';
    const engine = fakeEngine({
      parsePageSelection: async () => {
        throw failure;
      },
    });
    const reply = await handleRequest(engine, { id: 8, request: { kind: 'parsePageSelection', input: '9', totalPages: 3 } });
    expect(reply.message).toMatchObject({ type: 'error', id: 8, name: 'InvalidPageSelection', message: 'Page 9 does not exist' });
  });

  it('forwards SignaturesWouldBeInvalidated by name', async () => {
    const failure = new Error('the document is signed');
    failure.name = 'SignaturesWouldBeInvalidated';
    const engine = fakeEngine({
      saveToBytes: async () => {
        throw failure;
      },
    });
    const reply = await handleRequest(engine, {
      id: 9,
      request: { kind: 'save', document: 5, invalidateSignatures: false },
    });
    expect(reply.message).toMatchObject({ type: 'error', id: 9, name: 'SignaturesWouldBeInvalidated' });
  });

  it('answers the edit requests with the edited document, for the document they name', async () => {
    const calls: unknown[][] = [];
    const engine = fakeEngine({
      applyEdit: async (...args) => (calls.push(['applyEdit', ...args]), edited),
      undo: async (...args) => (calls.push(['undo', ...args]), edited),
      redo: async (...args) => (calls.push(['redo', ...args]), edited),
    });
    const edit = { kind: 'move', from: 0, count: 2, to: 3 } as const;
    expect(await handleRequest(engine, { id: 1, request: { kind: 'applyEdit', document: 5, edit } })).toEqual({
      message: { type: 'result', id: 1, value: edited },
      transfer: [],
    });
    await handleRequest(engine, { id: 2, request: { kind: 'undo', document: 5 } });
    await handleRequest(engine, { id: 3, request: { kind: 'redo', document: 5 } });
    expect(calls).toEqual([
      ['applyEdit', 5, edit],
      ['undo', 5],
      ['redo', 5],
    ]);
  });

  it('answers the import requests for the document and sources they name', async () => {
    const calls: unknown[][] = [];
    const prepared = { id: 3, pageCount: 2, warnings: ['w'] };
    const imported = { ...edited, sourceIds: [7] };
    const engine = fakeEngine({
      prepareImport: async (...args) => (calls.push(['prepareImport', ...args]), prepared),
      discardImport: async (...args) => void calls.push(['discardImport', ...args]),
      importPrepared: async (...args) => (calls.push(['importPrepared', ...args]), imported),
    });
    const bytes = new Uint8Array([1]);
    expect(await handleRequest(engine, { id: 1, request: { kind: 'prepareImport', bytes, password: 'pw' } })).toEqual({
      message: { type: 'result', id: 1, value: prepared },
      transfer: [],
    });
    expect(await handleRequest(engine, { id: 2, request: { kind: 'discardImport', id: 3 } })).toEqual({
      message: { type: 'result', id: 2, value: undefined },
      transfer: [],
    });
    expect(
      await handleRequest(engine, { id: 3, request: { kind: 'importPrepared', document: 5, ids: [3, 4], index: 1 } }),
    ).toEqual({ message: { type: 'result', id: 3, value: imported }, transfer: [] });
    expect(calls).toEqual([
      ['prepareImport', bytes, 'pw'],
      ['discardImport', 3],
      ['importPrepared', 5, [3, 4], 1],
    ]);
  });

  it('forwards a password failure of an import source by name', async () => {
    const failure = new Error('password needed');
    failure.name = 'PasswordRequired';
    const engine = fakeEngine({
      prepareImport: async () => {
        throw failure;
      },
    });
    const reply = await handleRequest(engine, { id: 6, request: { kind: 'prepareImport', bytes: new Uint8Array([1]) } });
    expect(reply.message).toMatchObject({ type: 'error', id: 6, name: 'PasswordRequired' });
  });

  it('answers the annotation requests for the document they name', async () => {
    const calls: unknown[][] = [];
    const highlight = {
      kind: 'highlight' as const,
      id: 0,
      pageIndex: 0,
      rect: { left: 1, top: 2, width: 3, height: 4 },
      color: { r: 255, g: 255, b: 0 },
    };
    const note = {
      pageIndex: 0,
      rect: { left: 1, top: 2, width: 3, height: 4 },
      contents: 'Hi',
      author: 'Ana',
      date: null,
      annotationId: null,
    };
    const layout = { fontSizePt: 12, lines: [{ text: 'Hi', xPt: 2, baselineFromTopPt: 11 }], overflow: false };
    const box = { left: 5, top: 6, width: 144, height: 72 };
    const engine = fakeEngine({
      annotations: async (...args) => (calls.push(['annotations', ...args]), [highlight]),
      comments: async (...args) => (calls.push(['comments', ...args]), [note]),
      freeTextLayout: async (...args) => (calls.push(['freeTextLayout', ...args]), layout),
      stampPlacement: async (...args) => (calls.push(['stampPlacement', ...args]), box),
    });
    const image = new Uint8Array([9]);
    const replies = [
      await handleRequest(engine, { id: 1, request: { kind: 'annotations', document: 5 } }),
      await handleRequest(engine, { id: 2, request: { kind: 'comments', document: 5 } }),
      await handleRequest(engine, { id: 3, request: { kind: 'freeTextLayout', contents: 'Hi', widthPt: 50, heightPt: 20 } }),
      await handleRequest(engine, {
        id: 4,
        request: { kind: 'stampPlacement', document: 5, pageIndex: 1, image, anchor: { x: 5, y: 6 } },
      }),
    ];
    expect(replies).toEqual([
      { message: { type: 'result', id: 1, value: [highlight] }, transfer: [] },
      { message: { type: 'result', id: 2, value: [note] }, transfer: [] },
      { message: { type: 'result', id: 3, value: layout }, transfer: [] },
      { message: { type: 'result', id: 4, value: box }, transfer: [] },
    ]);
    expect(calls).toEqual([
      ['annotations', 5],
      ['comments', 5],
      ['freeTextLayout', 'Hi', 50, 20],
      ['stampPlacement', 5, 1, image, { x: 5, y: 6 }],
    ]);
  });

  it('answers the form fields of the document it names, and forwards a fill and its refusal', async () => {
    const field = {
      kind: 'checkbox' as const,
      id: 1,
      pageIndex: 0,
      name: 'agree',
      rect: { left: 1, top: 2, width: 3, height: 4 },
      existing: true,
      checked: true,
    };
    const calls: unknown[][] = [];
    const failure = new Error('no form field with id 9');
    failure.name = 'FormFieldNotFound';
    const engine = fakeEngine({
      formFields: async (...args) => (calls.push(['formFields', ...args]), [field]),
      applyEdit: async (...args) => {
        calls.push(['applyEdit', ...args]);
        throw failure;
      },
    });
    const listed = await handleRequest(engine, { id: 1, request: { kind: 'formFields', document: 5 } });
    expect(listed).toEqual({ message: { type: 'result', id: 1, value: [field] }, transfer: [] });
    const edit = { kind: 'setFieldValue', fieldId: 9, value: { kind: 'checked', checked: false } } as const;
    const refused = await handleRequest(engine, { id: 2, request: { kind: 'applyEdit', document: 5, edit } });
    expect(refused.message).toMatchObject({ type: 'error', id: 2, name: 'FormFieldNotFound' });
    expect(calls).toEqual([
      ['formFields', 5],
      ['applyEdit', 5, edit],
    ]);
  });

  it('forwards EncodingGap by name, so a dialog can stay open', async () => {
    const failure = new Error("'日' cannot be shown in Helvetica (WinAnsi)");
    failure.name = 'EncodingGap';
    const engine = fakeEngine({
      freeTextLayout: async () => {
        throw failure;
      },
    });
    const reply = await handleRequest(engine, {
      id: 8,
      request: { kind: 'freeTextLayout', contents: '日', widthPt: 50, heightPt: 20 },
    });
    expect(reply.message).toMatchObject({ type: 'error', id: 8, name: 'EncodingGap' });
  });

  it('forwards EditNotPermitted by name', async () => {
    const failure = new Error('the document does not permit page assembly');
    failure.name = 'EditNotPermitted';
    const engine = fakeEngine({
      applyEdit: async () => {
        throw failure;
      },
    });
    const edit = { kind: 'remove', from: 0, count: 1 } as const;
    const reply = await handleRequest(engine, { id: 4, request: { kind: 'applyEdit', document: 5, edit } });
    expect(reply.message).toMatchObject({ type: 'error', id: 4, name: 'EditNotPermitted' });
  });
});
