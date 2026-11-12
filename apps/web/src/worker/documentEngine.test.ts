import { describe, expect, it } from 'vitest';
import { documentEngine, type CoreEngine } from './documentEngine';
import type { CoreGeometry, CorePageCharacters } from './pageText';

/** Placement that only marks its output, so a test sees it was applied. */
const geometry: CoreGeometry = {
  placeRects: (rects) => rects.map((value) => value + 1000),
  pointToPdf: (x, y) => new Float32Array([x, y]),
};

/** A core that keeps documents by id like core/pdf-wasm does. */
function fakeCore(options: { failDimensions?: boolean; restricted?: boolean } = {}) {
  const open = new Map<number, number>();
  const closed: number[] = [];
  const freed: string[] = [];
  let nextId = 0;
  const known = (doc: number) => {
    const marker = open.get(doc);
    if (marker === undefined) {
      const error = new Error(`document ${doc} is not open`);
      error.name = 'DocumentNotFound';
      throw error;
    }
    return marker;
  };
  const readable = (doc: number) => {
    known(doc);
    if (options.restricted) {
      const error = new Error('text extraction is not permitted');
      error.name = 'TextExtractionDenied';
      throw error;
    }
  };
  const core: CoreEngine = {
    open: (bytes) => {
      const id = nextId++;
      open.set(id, bytes[0]!);
      return id;
    },
    close: (doc) => {
      closed.push(doc);
      return open.delete(doc);
    },
    pageCount: (doc) => (known(doc), 1),
    pageDimensions: (doc) => {
      known(doc);
      if (options.failDimensions) throw new Error('dimensions failed');
      return [{ widthPt: 612, heightPt: 792, rotationDegrees: 0, free: () => {} }];
    },
    renderPage: (doc) => {
      // The "pixel" is the first byte of the document, so a test can tell
      // which document a render came from.
      const marker = known(doc);
      return { width: 1, height: 1, takeRgba: () => new Uint8Array([marker, 0, 0, 255]), free: () => {} };
    },
    textExtractionAllowed: (doc) => (known(doc), !options.restricted),
    search: (doc, query) => {
      readable(doc);
      return [{ pageIndex: 0, lines: new Float32Array([query.length, 0, 0, 0]), free: () => freed.push('match') }];
    },
    pageCharacters: (doc, page): CorePageCharacters => {
      readable(doc);
      // Ten characters on one line; the caret is the point's x.
      return {
        caretAt: (x) => Math.max(0, Math.min(10, Math.round(x))),
        wordAt: (x) => (x < 5 ? new Uint32Array([0, 4]) : undefined),
        textIn: (a, f) => 'abcdefghij'.slice(Math.min(a, f), Math.max(a, f)),
        rectsIn: (a, f) => new Float32Array([Math.min(a, f), 0, Math.abs(f - a), 1]),
        free: () => freed.push(`${doc}/${page}`),
      };
    },
  };
  return { core, closed, freed };
}

describe('documentEngine', () => {
  it('returns a handle per document and keeps an earlier one open', async () => {
    const { core, closed } = fakeCore();
    const engine = documentEngine(core, geometry);
    const a = await engine.open(new Uint8Array([10]));
    const b = await engine.open(new Uint8Array([20]));
    expect(a.handle).not.toBe(b.handle);
    expect(a.info).toEqual({
      pageCount: 1,
      pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }],
      textExtractionAllowed: true,
    });
    expect(closed).toEqual([]);
    // Each render reaches the document its handle names, not the newest one.
    expect((await engine.renderPage(a.handle, 0, 72)).rgba[0]).toBe(10);
    expect((await engine.renderPage(b.handle, 0, 72)).rgba[0]).toBe(20);
  });

  it('refuses to render a document once it was closed', async () => {
    const { core } = fakeCore();
    const engine = documentEngine(core, geometry);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.close(handle);
    await expect(engine.renderPage(handle, 0, 72)).rejects.toMatchObject({ name: 'DocumentNotFound' });
  });

  it('closes a document it opened but could not describe', async () => {
    const { core, closed } = fakeCore({ failDimensions: true });
    const engine = documentEngine(core, geometry);
    await expect(engine.open(new Uint8Array([10]))).rejects.toThrow('dimensions failed');
    expect(closed).toEqual([0]);
  });

  it('reports whether the document lets its text be read', async () => {
    const { core } = fakeCore({ restricted: true });
    const { info } = await documentEngine(core, geometry).open(new Uint8Array([10]));
    expect(info.textExtractionAllowed).toBe(false);
  });

  it('places search matches on the drawn page and frees them', async () => {
    const { core, freed } = fakeCore();
    const engine = documentEngine(core, geometry);
    const { handle } = await engine.open(new Uint8Array([10]));
    const hits = await engine.search(handle, 'beta');
    expect(hits).toEqual([{ pageIndex: 0, rects: new Float32Array([1004, 1000, 1000, 1000]) }]);
    expect(freed).toEqual(['match']);
  });

  it('selects the text between two points and the word under one', async () => {
    const { core } = fakeCore();
    const engine = documentEngine(core, geometry);
    const { handle } = await engine.open(new Uint8Array([10]));
    // Dragging backwards selects the same text.
    expect(await engine.selectText(handle, 0, { x: 7, y: 0 }, { x: 2, y: 0 })).toEqual({
      text: 'cdefg',
      rects: new Float32Array([1002, 1000, 1005, 1001]),
    });
    expect((await engine.selectWord(handle, 0, { x: 1, y: 0 })).text).toBe('abcd');
    // A click (no drag) and whitespace select nothing.
    expect((await engine.selectText(handle, 0, { x: 3, y: 0 }, { x: 3, y: 0 })).text).toBe('');
    expect((await engine.selectWord(handle, 0, { x: 8, y: 0 })).text).toBe('');
  });

  it("builds a page's characters once and frees them when the document closes", async () => {
    const { core, freed } = fakeCore();
    let built = 0;
    const pageCharacters = core.pageCharacters;
    core.pageCharacters = (doc, page) => (built++, pageCharacters(doc, page));
    const engine = documentEngine(core, geometry);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.selectText(handle, 0, { x: 1, y: 0 }, { x: 2, y: 0 });
    await engine.selectText(handle, 0, { x: 1, y: 0 }, { x: 4, y: 0 });
    expect(built).toBe(1);
    await engine.close(handle);
    expect(freed).toEqual([`${handle}/0`]);
  });

  it('refuses every text read of a restricted document as TextExtractionDenied', async () => {
    const { core } = fakeCore({ restricted: true });
    const engine = documentEngine(core, geometry);
    const { handle } = await engine.open(new Uint8Array([10]));
    const denied = { name: 'TextExtractionDenied' };
    await expect(engine.search(handle, 'beta')).rejects.toMatchObject(denied);
    await expect(engine.selectText(handle, 0, { x: 1, y: 0 }, { x: 2, y: 0 })).rejects.toMatchObject(denied);
    await expect(engine.selectWord(handle, 0, { x: 1, y: 0 })).rejects.toMatchObject(denied);
  });

  it('refuses a text read of a closed document or of a page past the end', async () => {
    const { core } = fakeCore();
    const engine = documentEngine(core, geometry);
    const { handle } = await engine.open(new Uint8Array([10]));
    await expect(engine.selectWord(handle, 3, { x: 1, y: 0 })).rejects.toMatchObject({ name: 'PageIndexOutOfBounds' });
    await engine.close(handle);
    await expect(engine.search(handle, 'beta')).rejects.toMatchObject({ name: 'DocumentNotFound' });
  });
});
