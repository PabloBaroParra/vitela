import { describe, expect, it } from 'vitest';
import { EngineClient } from './client.js';
import { EngineError } from './errors.js';
import type { EngineResults, MessagePortLike, RequestEnvelope, WorkerMessage } from './protocol.js';

/** A port whose sent messages are recorded and whose replies the test drives. */
function fakePort() {
  const sent: { message: RequestEnvelope; transfer?: Transferable[] }[] = [];
  const listeners: ((event: MessageEvent<WorkerMessage>) => void)[] = [];
  const port: MessagePortLike<WorkerMessage, RequestEnvelope> = {
    postMessage: (message, transfer) => sent.push({ message, transfer }),
    addEventListener: (_type, listener) => listeners.push(listener),
  };
  const reply = (data: WorkerMessage) =>
    listeners.forEach((listener) => listener(new MessageEvent('message', { data })));
  return { port, sent, reply };
}

describe('EngineClient', () => {
  it('resolves ready when the worker reports ready', async () => {
    const { port, reply } = fakePort();
    const client = new EngineClient(port);
    reply({ type: 'ready' });
    await expect(client.ready).resolves.toBeUndefined();
  });

  it('rejects ready and every later call when the worker reports a fatal error', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    reply({ type: 'fatal', message: 'pdfium runtime aborted' });
    await expect(client.ready).rejects.toThrow('pdfium runtime aborted');
    await expect(client.open(new Uint8Array([1]))).rejects.toThrow('pdfium runtime aborted');
    expect(sent).toHaveLength(0);
  });

  it('transfers the document bytes and resolves open with the matching reply', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const bytes = new Uint8Array([37, 80, 68, 70]);
    const opened = client.open(bytes);

    expect(sent).toHaveLength(1);
    const [{ message, transfer }] = sent as [(typeof sent)[number]];
    expect(message.request).toEqual({ kind: 'open', bytes, password: undefined });
    expect(transfer).toEqual([bytes.buffer]);

    const pages = [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }];
    const value = {
      handle: 3,
      info: {
        pageCount: 1,
        pages,
        pageIds: [0],
        textExtractionAllowed: true,
        documentAssemblyAllowed: true,
        blocks: [],
        importRefusal: null,
        annotationEditingAllowed: true,
        formFillAllowed: true,
      },
    };
    reply({ type: 'result', id: message.id, value });
    await expect(opened).resolves.toEqual(value);
  });

  it('correlates out-of-order replies by request id', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const first = client.renderPage(0, 0, 100);
    const second = client.renderPage(0, 1, 200);
    const [a, b] = sent.map((s) => s.message.id) as [number, number];
    expect(a).not.toBe(b);

    const page = { width: 2, height: 1, rgba: new Uint8ClampedArray(8) };
    reply({ type: 'error', id: b, name: 'PageIndexOutOfBounds', message: 'page 1: out of range' });
    reply({ type: 'result', id: a, value: page });

    await expect(first).resolves.toBe(page);
    await expect(second).rejects.toThrow('page 1: out of range');
  });

  it('fails pending and future requests when the worker dies', async () => {
    const { port, sent } = fakePort();
    const client = new EngineClient(port);
    const pending = client.renderPage(0, 0, 100);
    client.fail('worker script did not load');
    await expect(pending).rejects.toThrow('worker script did not load');
    await expect(client.ready).rejects.toThrow('worker script did not load');
    await expect(client.renderPage(0, 0, 100)).rejects.toThrow('worker script did not load');
    expect(sent).toHaveLength(1);
  });

  it('terminates the worker: pending and later requests fail, and a ready still pending rejects', async () => {
    const { port, sent } = fakePort();
    let terminated = 0;
    const client = new EngineClient({ ...port, terminate: () => void terminated++ });
    const pending = client.renderPage(0, 0, 100);
    client.terminate();
    expect(terminated).toBe(1);
    await expect(pending).rejects.toThrow('the engine was terminated');
    await expect(client.ready).rejects.toThrow('the engine was terminated');
    await expect(client.close(0)).rejects.toThrow('the engine was terminated');
    expect(sent).toHaveLength(1);
    // A second terminate does nothing more.
    client.terminate();
    expect(terminated).toBe(1);
  });

  it('terminating a port that cannot be terminated still stops the client', async () => {
    const { port } = fakePort();
    const client = new EngineClient(port);
    client.terminate();
    await expect(client.open(new Uint8Array([1]))).rejects.toThrow('the engine was terminated');
  });

  it('ignores replies for ids it never sent', () => {
    const { port, reply } = fakePort();
    new EngineClient(port);
    expect(() => reply({ type: 'result', id: 999, value: undefined })).not.toThrow();
  });

  it('sends the password with an open request', () => {
    const { port, sent } = fakePort();
    const client = new EngineClient(port);
    const bytes = new Uint8Array([1]);
    void client.open(bytes, 'user-aes-pass');
    expect(sent[0]!.message.request).toEqual({ kind: 'open', bytes, password: 'user-aes-pass' });
  });

  it('names the document in every render and close request', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    void client.renderPage(4, 2, 96);
    const closed = client.close(4);
    expect(sent.map((s) => s.message.request)).toEqual([
      { kind: 'renderPage', document: 4, pageIndex: 2, dpi: 96 },
      { kind: 'close', document: 4 },
    ]);
    reply({ type: 'result', id: sent[1]!.message.id, value: undefined });
    await expect(closed).resolves.toBeUndefined();
  });

  it('names the document and page in every text request', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const found = client.search(4, 'beta');
    void client.selectText(4, 1, { x: 1, y: 2 }, { x: 3, y: 4 });
    void client.selectWord(4, 2, { x: 5, y: 6 });
    expect(sent.map((s) => s.message.request)).toEqual([
      { kind: 'search', document: 4, query: 'beta' },
      { kind: 'selectText', document: 4, pageIndex: 1, anchor: { x: 1, y: 2 }, focus: { x: 3, y: 4 } },
      { kind: 'selectWord', document: 4, pageIndex: 2, point: { x: 5, y: 6 } },
    ]);
    const hits = [{ pageIndex: 0, rects: new Float32Array([1, 2, 3, 4]) }];
    reply({ type: 'result', id: sent[0]!.message.id, value: hits });
    await expect(found).resolves.toBe(hits);
  });

  it('rejects a text read of a restricted document as TextExtractionDenied', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const found = client.search(4, 'beta');
    reply({ type: 'error', id: sent[0]!.message.id, name: 'TextExtractionDenied', message: 'text extraction is not permitted' });
    await expect(found).rejects.toMatchObject({ name: 'TextExtractionDenied' });
  });

  it('rejects with an EngineError that carries the forwarded name', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const opened = client.open(new Uint8Array([1]));
    reply({ type: 'error', id: sent[0]!.message.id, name: 'PasswordRequired', message: 'needs a password' });

    const error = await opened.catch((reason: unknown) => reason);
    expect(error).toBeInstanceOf(EngineError);
    expect(error).toMatchObject({ name: 'PasswordRequired', message: 'needs a password' });
  });

  it('asks for the saved bytes of a document, saying whether signatures may be invalidated', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const saved = client.saveToBytes(4, true);
    expect(sent[0]!.message.request).toEqual({ kind: 'save', document: 4, invalidateSignatures: true });
    const bytes = new Uint8Array([37, 80, 68, 70]);
    reply({ type: 'result', id: sent[0]!.message.id, value: bytes });
    await expect(saved).resolves.toBe(bytes);
  });

  it('sends the compress requests and resolves with their replies', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const presets = client.compressPresets();
    const refusal = client.compressionRefusal(4);
    const signed = client.compressionInvalidatesSignatures(4);
    const compressed = client.saveCompressedToBytes(4, 'small', true);
    expect(sent.map(({ message }) => message.request)).toEqual([
      { kind: 'compressPresets' },
      { kind: 'compressionRefusal', document: 4 },
      { kind: 'compressionInvalidatesSignatures', document: 4 },
      { kind: 'saveCompressed', document: 4, preset: 'small', invalidateSignatures: true },
    ]);
    const document = {
      bytes: new Uint8Array([37]),
      beforeBytes: 10,
      afterBytes: 1,
      savedBytes: 9,
      reduced: true,
      refusals: [],
    };
    const ids = sent.map(({ message }) => message.id);
    reply({ type: 'result', id: ids[0]!, value: ['lossless', 'balanced', 'small'] });
    reply({ type: 'result', id: ids[1]!, value: null });
    reply({ type: 'result', id: ids[2]!, value: true });
    reply({ type: 'result', id: ids[3]!, value: document });
    await expect(presets).resolves.toEqual(['lossless', 'balanced', 'small']);
    await expect(refusal).resolves.toBeNull();
    await expect(signed).resolves.toBe(true);
    await expect(compressed).resolves.toBe(document);
  });

  it('sends the protect requests and resolves with their replies', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const refusal = client.protectionRefusal(4);
    const signed = client.protectionInvalidatesSignatures(4);
    const protectedBytes = client.protectToBytes(4, { openPassword: 'open-pw', permissionsPassword: 'perm-pw' }, true);
    expect(sent.map(({ message }) => message.request)).toEqual([
      { kind: 'protectionRefusal', document: 4 },
      { kind: 'protectionInvalidatesSignatures', document: 4 },
      { kind: 'protect', document: 4, openPassword: 'open-pw', permissionsPassword: 'perm-pw', invalidateSignatures: true },
    ]);
    const bytes = new Uint8Array([37, 80, 68, 70]);
    const ids = sent.map(({ message }) => message.id);
    reply({ type: 'result', id: ids[0]!, value: null });
    reply({ type: 'result', id: ids[1]!, value: true });
    reply({ type: 'result', id: ids[2]!, value: bytes });
    await expect(refusal).resolves.toBeNull();
    await expect(signed).resolves.toBe(true);
    await expect(protectedBytes).resolves.toBe(bytes);
  });

  it('sends the watermark requests and resolves with their replies', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const defaults = client.watermarkDefaults();
    const refusal = client.watermarkRefusal(4);
    expect(sent.map(({ message }) => message.request)).toEqual([
      { kind: 'watermarkDefaults' },
      { kind: 'watermarkRefusal', document: 4 },
    ]);
    const look = {
      fontSize: 60,
      textOpacity: 0.3,
      textRotationDegrees: 45,
      textBehind: false,
      pictureScale: 0.5,
      pictureOpacity: 0.3,
      pictureRotationDegrees: 0,
      pictureBehind: true,
    };
    const ids = sent.map(({ message }) => message.id);
    reply({ type: 'result', id: ids[0]!, value: look });
    reply({ type: 'result', id: ids[1]!, value: null });
    await expect(defaults).resolves.toEqual(look);
    await expect(refusal).resolves.toBeNull();
  });

  it('sends the document-properties requests and resolves with their replies', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const read = client.documentProperties(4);
    const refusal = client.metadataRefusal(4);
    expect(sent.map(({ message }) => message.request)).toEqual([
      { kind: 'documentProperties', document: 4 },
      { kind: 'metadataRefusal', document: 4 },
    ]);
    const properties = { title: 'Report', author: null, subject: null, keywords: null, creator: null, producer: null };
    const ids = sent.map(({ message }) => message.id);
    reply({ type: 'result', id: ids[0]!, value: properties });
    reply({ type: 'result', id: ids[1]!, value: 'no' });
    await expect(read).resolves.toEqual(properties);
    await expect(refusal).resolves.toBe('no');
  });

  it('sends the extract and split requests and resolves with their replies', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const refusal = client.extractionRefusal(4);
    const signed = client.extractSourceIsSigned(4);
    const extracted = client.extractPages(4, [1, 2]);
    const plan = client.planSplit('2,4', 5, 'report.pdf');
    expect(sent.map(({ message }) => message.request)).toEqual([
      { kind: 'extractionRefusal', document: 4 },
      { kind: 'extractSourceIsSigned', document: 4 },
      { kind: 'extract', document: 4, pages: [1, 2] },
      { kind: 'planSplit', cuts: '2,4', totalPages: 5, documentName: 'report.pdf' },
    ]);
    const bytes = new Uint8Array([37, 80, 68, 70]);
    const parts = [{ first: 0, last: 1, fileName: 'report-part1.pdf' }];
    const ids = sent.map(({ message }) => message.id);
    reply({ type: 'result', id: ids[0]!, value: null });
    reply({ type: 'result', id: ids[1]!, value: false });
    reply({ type: 'result', id: ids[2]!, value: bytes });
    reply({ type: 'result', id: ids[3]!, value: parts });
    await expect(refusal).resolves.toBeNull();
    await expect(signed).resolves.toBe(false);
    await expect(extracted).resolves.toBe(bytes);
    await expect(plan).resolves.toBe(parts);
  });

  it('sends the image-export requests and resolves with their replies', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const requests = [
      client.imageExportFormats(),
      client.parsePageSelection('1-2', 3),
      client.pageImageFileNames('r.pdf', [0, 1], 3, 'png'),
      client.firstPageTooLargeToExport(4, [0, 1], 150),
      client.openImageExport(4),
      client.exportPageImage(7, 1, 150, 'jpeg'),
      client.closeImageExport(7),
    ];
    expect(sent.map(({ message }) => message.request)).toEqual([
      { kind: 'imageExportFormats' },
      { kind: 'parsePageSelection', input: '1-2', totalPages: 3 },
      { kind: 'pageImageFileNames', documentName: 'r.pdf', pages: [0, 1], totalPages: 3, format: 'png' },
      { kind: 'firstPageTooLargeToExport', document: 4, pages: [0, 1], dpi: 150 },
      { kind: 'openImageExport', document: 4 },
      { kind: 'exportPageImage', export: 7, pageIndex: 1, dpi: 150, format: 'jpeg' },
      { kind: 'closeImageExport', export: 7 },
    ]);
    const image = new Uint8Array([0xff, 0xd8]);
    const values: EngineResults[keyof EngineResults][] = [['png', 'jpeg'], [0, 1], ['r-1.png', 'r-2.png'], null, { handle: 7, pageCount: 3 }, image, true];
    sent.forEach(({ message }, i) => reply({ type: 'result', id: message.id, value: values[i] }));
    for (const [i, request] of requests.entries()) await expect(request).resolves.toEqual(values[i]);
  });

  it('rejects a save that would break a signature with that name', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const saved = client.saveToBytes(4, false);
    reply({ type: 'error', id: sent[0]!.message.id, name: 'SignaturesWouldBeInvalidated', message: 'signed' });
    await expect(saved).rejects.toMatchObject({ name: 'SignaturesWouldBeInvalidated' });
  });

  it('names the document in every edit request and resolves with the edited document', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const edited = client.applyEdit(4, { kind: 'rotate', from: 1, count: 2, degrees: 90 });
    void client.undo(4);
    void client.redo(4);
    expect(sent.map((s) => s.message.request)).toEqual([
      { kind: 'applyEdit', document: 4, edit: { kind: 'rotate', from: 1, count: 2, degrees: 90 } },
      { kind: 'undo', document: 4 },
      { kind: 'redo', document: 4 },
    ]);
    const info = {
      pageCount: 0,
      pages: [],
      pageIds: [],
      textExtractionAllowed: true,
      documentAssemblyAllowed: true,
      blocks: [],
      importRefusal: null,
      annotationEditingAllowed: true,
      formFillAllowed: true,
    };
    const value = { info, canUndo: true, canRedo: false, annotations: [], addedAnnotationId: null, formFields: [] };
    reply({ type: 'result', id: sent[0]!.message.id, value });
    await expect(edited).resolves.toBe(value);
  });

  it('transfers an import source, and names the document and sources it imports', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const bytes = new Uint8Array([1, 2]);
    const prepared = client.prepareImport(bytes, 'pw');
    void client.discardImport(3);
    void client.importPrepared(4, [1, 2], 0);
    expect(sent.map((s) => s.message.request)).toEqual([
      { kind: 'prepareImport', bytes, password: 'pw' },
      { kind: 'discardImport', id: 3 },
      { kind: 'importPrepared', document: 4, ids: [1, 2], index: 0 },
    ]);
    expect(sent[0]!.transfer).toEqual([bytes.buffer]);
    reply({ type: 'error', id: sent[0]!.message.id, name: 'WrongPassword', message: 'wrong' });
    await expect(prepared).rejects.toMatchObject({ name: 'WrongPassword' });
  });

  it('sends an annotation edit and its stamp image as they are, copied rather than transferred', () => {
    const { port, sent } = fakePort();
    const client = new EngineClient(port);
    const image = new Uint8Array([1, 2, 3]);
    const edit = {
      kind: 'addStamp',
      page: 0,
      rect: { left: 1, top: 2, width: 3, height: 4 },
      image,
    } as const;
    void client.applyEdit(2, edit);
    void client.stampPlacement(2, 0, image, { x: 5, y: 6 });
    expect(sent.map((s) => s.message.request)).toEqual([
      { kind: 'applyEdit', document: 2, edit },
      { kind: 'stampPlacement', document: 2, pageIndex: 0, image, anchor: { x: 5, y: 6 } },
    ]);
    expect(sent.map((s) => s.transfer)).toEqual([[], []]);
    expect(image.byteLength).toBe(3);
  });

  it('asks for the form fields and sends a fill as it is', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const listed = client.formFields(3);
    const edit = { kind: 'setFieldValue', fieldId: 0, value: { kind: 'text', text: 'José' } } as const;
    void client.applyEdit(3, edit);
    expect(sent.map((s) => s.message.request)).toEqual([
      { kind: 'formFields', document: 3 },
      { kind: 'applyEdit', document: 3, edit },
    ]);
    reply({ type: 'result', id: sent[0]!.message.id, value: [] });
    await expect(listed).resolves.toEqual([]);
  });

  it('asks for the annotations, the comments and a free-text layout', async () => {
    const { port, sent, reply } = fakePort();
    const client = new EngineClient(port);
    const listed = client.annotations(7);
    void client.comments(7);
    const layout = client.freeTextLayout('Hello', 120, 30);
    expect(sent.map((s) => s.message.request)).toEqual([
      { kind: 'annotations', document: 7 },
      { kind: 'comments', document: 7 },
      { kind: 'freeTextLayout', contents: 'Hello', widthPt: 120, heightPt: 30 },
    ]);
    reply({ type: 'error', id: sent[2]!.message.id, name: 'EncodingGap', message: "'日' cannot be shown" });
    reply({ type: 'result', id: sent[0]!.message.id, value: [] });
    await expect(listed).resolves.toEqual([]);
    await expect(layout).rejects.toMatchObject({ name: 'EncodingGap' });
  });
});
