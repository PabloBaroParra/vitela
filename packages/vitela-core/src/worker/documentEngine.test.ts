import { describe, expect, it } from 'vitest';
import { documentEngine, type CoreEdits, type CoreEngine } from './documentEngine.js';
import type { CoreGeometry, CorePageCharacters } from './pageText.js';

/** Placement that only marks its output, so a test sees it was applied. */
const geometry: CoreGeometry = {
  placeRects: (rects) => rects.map((value) => value + 1000),
  pointToPdf: (x, y) => new Float32Array([x, y]),
};

/** One fake page: its identity, size and the PDF it came from (0: the opened one). */
interface FakePage {
  readonly id: number;
  readonly widthPt: number;
  readonly rotationDegrees: number;
  readonly source?: number;
}

/** What a fake `EditCommand` carries; the real one is an opaque wasm object. */
type FakeEdit =
  | { kind: 'move'; from: number; count: number; to: number; free(): void }
  | { kind: 'rotate'; from: number; count: number; delta: number; free(): void }
  | { kind: 'remove'; from: number; count: number; free(): void };

type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;

/** A core that keeps documents by id like core/pdf-wasm does. */
function fakeCore(
  options: { failDimensions?: boolean; restricted?: boolean; pages?: number; assembly?: boolean; failRefresh?: boolean } = {},
) {
  interface Doc {
    marker: number;
    pages: FakePage[];
    undo: FakePage[][];
    redo: FakePage[][];
  }
  const open = new Map<number, Doc>();
  const closed: number[] = [];
  const freed: string[] = [];
  const saves: [number, boolean][] = [];
  const refreshed: number[] = [];
  /** Prepared import sources: id -> page count. */
  const prepared = new Map<number, number>();
  let nextId = 0;
  let nextPrepared = 1;
  let nextSource = 1;
  let nextPageId = 200;
  const known = (doc: number) => {
    const entry = open.get(doc);
    if (entry === undefined) {
      const error = new Error(`document ${doc} is not open`);
      error.name = 'DocumentNotFound';
      throw error;
    }
    return entry;
  };
  const readable = (doc: number) => {
    known(doc);
    if (options.restricted) {
      const error = new Error('text extraction is not permitted');
      error.name = 'TextExtractionDenied';
      throw error;
    }
  };
  const command = (edit: DistributiveOmit<FakeEdit, 'free'>): FakeEdit =>
    ({ ...edit, free: () => freed.push(`edit:${edit.kind}`) }) as FakeEdit;
  const edits: CoreEdits = {
    movePages: (from, count, to) => command({ kind: 'move', from, count, to }),
    rotatePages: (from, count, delta) => command({ kind: 'rotate', from, count, delta }),
    removePages: (from, count) => command({ kind: 'remove', from, count }),
  };
  const core: CoreEngine = {
    open: (bytes) => {
      const id = nextId++;
      const pages = Array.from({ length: options.pages ?? 1 }, (_, index) => ({
        id: 100 + index,
        widthPt: 612,
        rotationDegrees: 0,
      }));
      open.set(id, { marker: bytes[0]!, pages, undo: [], redo: [] });
      return id;
    },
    close: (doc) => {
      closed.push(doc);
      return open.delete(doc);
    },
    pageCount: (doc) => known(doc).pages.length,
    pageDimensions: (doc) => {
      const { pages } = known(doc);
      if (options.failDimensions) throw new Error('dimensions failed');
      return pages.map((page) => {
        const turned = page.rotationDegrees % 180 !== 0;
        return {
          widthPt: turned ? 792 : page.widthPt,
          heightPt: turned ? page.widthPt : 792,
          rotationDegrees: page.rotationDegrees,
          free: () => {},
        };
      });
    },
    pageIds: (doc) => new Uint32Array(known(doc).pages.map((page) => page.id)),
    documentAssemblyAllowed: (doc) => (known(doc), options.assembly ?? true),
    renderPage: (doc) => {
      // The "pixel" is the first byte of the document, so a test can tell
      // which document a render came from.
      const { marker } = known(doc);
      return { width: 1, height: 1, takeRgba: () => new Uint8Array([marker, 0, 0, 255]), free: () => {} };
    },
    textExtractionAllowed: (doc) => (known(doc), !options.restricted),
    saveToBytes: (doc, invalidate) => {
      const { marker } = known(doc);
      saves.push([doc, invalidate]);
      return new Uint8Array([marker, invalidate ? 1 : 0]);
    },
    applyEdit: (doc, raw) => {
      const entry = known(doc);
      if (options.assembly === false) {
        const error = new Error('the document does not permit page assembly');
        error.name = 'EditNotPermitted';
        throw error;
      }
      const edit = raw as FakeEdit;
      const pages = [...entry.pages];
      if (edit.kind === 'move') {
        const block = pages.splice(edit.from, edit.count);
        pages.splice(edit.to, 0, ...block);
      } else if (edit.kind === 'rotate') {
        for (let i = edit.from; i < edit.from + edit.count; i++) {
          pages[i] = { ...pages[i]!, rotationDegrees: (pages[i]!.rotationDegrees + edit.delta + 360) % 360 };
        }
      } else {
        pages.splice(edit.from, edit.count);
      }
      entry.undo.push(entry.pages);
      entry.redo = [];
      entry.pages = pages;
    },
    undo: (doc) => {
      const entry = known(doc);
      const previous = entry.undo.pop();
      if (!previous) return false;
      entry.redo.push(entry.pages);
      entry.pages = previous;
      return true;
    },
    redo: (doc) => {
      const entry = known(doc);
      const next = entry.redo.pop();
      if (!next) return false;
      entry.undo.push(entry.pages);
      entry.pages = next;
      return true;
    },
    prepareImport: (bytes, password) => {
      // A source whose first byte is 0xee is locked under "secret".
      if (bytes[0] === 0xee && password !== 'secret') {
        const error = new Error('password needed');
        error.name = password === undefined ? 'PasswordRequired' : 'WrongPassword';
        throw error;
      }
      const id = nextPrepared++;
      prepared.set(id, bytes.length);
      return { id, pageCount: bytes.length, warnings: ['a link was dropped'], free: () => freed.push('prepared') };
    },
    discardPrepared: (id) => prepared.delete(id),
    importPrepared: (doc, ids, index) => {
      const entry = known(doc);
      if (options.assembly === false) {
        const error = new Error('the document does not permit page assembly');
        error.name = 'EditNotPermitted';
        throw error;
      }
      const added: FakePage[] = [];
      const sourceIds: number[] = [];
      for (const id of ids) {
        const count = prepared.get(id)!;
        prepared.delete(id);
        const source = nextSource++;
        sourceIds.push(source);
        for (let i = 0; i < count; i++) added.push({ id: nextPageId++, widthPt: 300, rotationDegrees: 0, source });
      }
      const pages = [...entry.pages];
      pages.splice(index, 0, ...added);
      entry.undo.push(entry.pages);
      entry.redo = [];
      entry.pages = pages;
      return { pageCount: added.length, sourceIds: new Uint32Array(sourceIds), free: () => freed.push('imported') };
    },
    importRefusal: (doc) => (known(doc), options.assembly === false ? 'no assembly' : undefined),
    documentBlocks: (doc) => {
      const blocks: { source: string; sourceId: number | undefined; part: undefined; start: number; count: number; free(): void }[] = [];
      known(doc).pages.forEach((page, index) => {
        const source = page.source ?? 0;
        const last = blocks.at(-1);
        if (last && (last.sourceId ?? 0) === source) last.count++;
        else
          blocks.push({
            source: source === 0 ? 'base' : 'imported',
            sourceId: source === 0 ? undefined : source,
            part: undefined,
            start: index,
            count: 1,
            free: () => {},
          });
      });
      return blocks;
    },
    canUndo: (doc) => known(doc).undo.length > 0,
    canRedo: (doc) => known(doc).redo.length > 0,
    refreshPreview: (doc) => {
      known(doc);
      if (options.failRefresh) throw new Error('preview failed');
      refreshed.push(doc);
    },
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
  return { core, edits, closed, freed, saves, refreshed, prepared };
}

describe('documentEngine', () => {
  it('returns a handle per document and keeps an earlier one open', async () => {
    const { core, edits, closed } = fakeCore();
    const engine = documentEngine(core, geometry, edits);
    const a = await engine.open(new Uint8Array([10]));
    const b = await engine.open(new Uint8Array([20]));
    expect(a.handle).not.toBe(b.handle);
    expect(a.info).toEqual({
      pageCount: 1,
      pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }],
      pageIds: [100],
      textExtractionAllowed: true,
      documentAssemblyAllowed: true,
      blocks: [{ source: { kind: 'base' }, part: null, start: 0, count: 1 }],
      importRefusal: null,
    });
    expect(closed).toEqual([]);
    // Each render reaches the document its handle names, not the newest one.
    expect((await engine.renderPage(a.handle, 0, 72)).rgba[0]).toBe(10);
    expect((await engine.renderPage(b.handle, 0, 72)).rgba[0]).toBe(20);
  });

  it('saves the document its handle names, passing the signature acknowledgement', async () => {
    const { core, edits, saves } = fakeCore();
    const engine = documentEngine(core, geometry, edits);
    const a = await engine.open(new Uint8Array([10]));
    const b = await engine.open(new Uint8Array([20]));
    expect(await engine.saveToBytes(b.handle, false)).toEqual(new Uint8Array([20, 0]));
    expect(await engine.saveToBytes(a.handle, true)).toEqual(new Uint8Array([10, 1]));
    expect(saves).toEqual([
      [b.handle, false],
      [a.handle, true],
    ]);
  });

  it('refuses to save a document once it was closed', async () => {
    const { core, edits } = fakeCore();
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.close(handle);
    await expect(engine.saveToBytes(handle, false)).rejects.toMatchObject({ name: 'DocumentNotFound' });
  });

  it('refuses to render a document once it was closed', async () => {
    const { core, edits } = fakeCore();
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.close(handle);
    await expect(engine.renderPage(handle, 0, 72)).rejects.toMatchObject({ name: 'DocumentNotFound' });
  });

  it('closes a document it opened but could not describe', async () => {
    const { core, edits, closed } = fakeCore({ failDimensions: true });
    const engine = documentEngine(core, geometry, edits);
    await expect(engine.open(new Uint8Array([10]))).rejects.toThrow('dimensions failed');
    expect(closed).toEqual([0]);
  });

  it('reports whether the document lets its text be read', async () => {
    const { core, edits } = fakeCore({ restricted: true });
    const { info } = await documentEngine(core, geometry, edits).open(new Uint8Array([10]));
    expect(info.textExtractionAllowed).toBe(false);
  });

  it('places search matches on the drawn page and frees them', async () => {
    const { core, edits, freed } = fakeCore();
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    const hits = await engine.search(handle, 'beta');
    expect(hits).toEqual([{ pageIndex: 0, rects: new Float32Array([1004, 1000, 1000, 1000]) }]);
    expect(freed).toEqual(['match']);
  });

  it('selects the text between two points and the word under one', async () => {
    const { core, edits } = fakeCore();
    const engine = documentEngine(core, geometry, edits);
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
    const { core, edits, freed } = fakeCore();
    let built = 0;
    const pageCharacters = core.pageCharacters;
    core.pageCharacters = (doc, page) => (built++, pageCharacters(doc, page));
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.selectText(handle, 0, { x: 1, y: 0 }, { x: 2, y: 0 });
    await engine.selectText(handle, 0, { x: 1, y: 0 }, { x: 4, y: 0 });
    expect(built).toBe(1);
    await engine.close(handle);
    expect(freed).toEqual([`${handle}/0`]);
  });

  it('refuses every text read of a restricted document as TextExtractionDenied', async () => {
    const { core, edits } = fakeCore({ restricted: true });
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    const denied = { name: 'TextExtractionDenied' };
    await expect(engine.search(handle, 'beta')).rejects.toMatchObject(denied);
    await expect(engine.selectText(handle, 0, { x: 1, y: 0 }, { x: 2, y: 0 })).rejects.toMatchObject(denied);
    await expect(engine.selectWord(handle, 0, { x: 1, y: 0 })).rejects.toMatchObject(denied);
  });

  it('refuses a text read of a closed document or of a page past the end', async () => {
    const { core, edits } = fakeCore();
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await expect(engine.selectWord(handle, 3, { x: 1, y: 0 })).rejects.toMatchObject({ name: 'PageIndexOutOfBounds' });
    await engine.close(handle);
    await expect(engine.search(handle, 'beta')).rejects.toMatchObject({ name: 'DocumentNotFound' });
  });

  it('reports whether the document lets its pages be reassembled', async () => {
    const { core, edits } = fakeCore({ assembly: false });
    const { info } = await documentEngine(core, geometry, edits).open(new Uint8Array([10]));
    expect(info.documentAssemblyAllowed).toBe(false);
  });

  it('applies a page edit, refreshes the preview and answers with the edited pages', async () => {
    const { core, edits, freed, refreshed } = fakeCore({ pages: 3 });
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));

    const moved = await engine.applyEdit(handle, { kind: 'move', from: 0, count: 1, to: 2 });
    expect(moved.info.pageIds).toEqual([101, 102, 100]);
    expect(moved.info.pageCount).toBe(3);
    expect(moved).toMatchObject({ canUndo: true, canRedo: false });
    // Rendering reads the preview: it must show the edit before anything paints.
    expect(refreshed).toEqual([handle]);
    expect(freed).toContain('edit:move');

    const turned = await engine.applyEdit(handle, { kind: 'rotate', from: 1, count: 2, degrees: 90 });
    expect(turned.info.pages.map((page) => page.rotationDegrees)).toEqual([0, 90, 90]);
    expect(turned.info.pages[1]).toEqual({ widthPt: 792, heightPt: 612, rotationDegrees: 90 });

    const removed = await engine.applyEdit(handle, { kind: 'remove', from: 0, count: 2 });
    expect(removed.info.pageIds).toEqual([100]);
    expect(refreshed).toHaveLength(3);
  });

  it('steps back and forth through the edits, refreshing the preview each time', async () => {
    const { core, edits, refreshed } = fakeCore({ pages: 2 });
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.applyEdit(handle, { kind: 'remove', from: 0, count: 1 });

    const undone = await engine.undo(handle);
    expect(undone).toMatchObject({ canUndo: false, canRedo: true });
    expect(undone.info.pageIds).toEqual([100, 101]);
    const redone = await engine.redo(handle);
    expect(redone).toMatchObject({ canUndo: true, canRedo: false });
    expect(redone.info.pageIds).toEqual([101]);
    expect(refreshed).toHaveLength(3);
  });

  it('places text against the edited page sizes and forgets the stale characters', async () => {
    const { core, edits, freed } = fakeCore({ pages: 2 });
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.selectText(handle, 1, { x: 1, y: 0 }, { x: 2, y: 0 });
    await engine.applyEdit(handle, { kind: 'remove', from: 0, count: 1 });
    // The characters of the old page 2 describe a page that moved.
    expect(freed).toContain(`${handle}/1`);
    await expect(engine.selectWord(handle, 1, { x: 1, y: 0 })).rejects.toMatchObject({ name: 'PageIndexOutOfBounds' });
  });

  it('passes a refusal through and frees the command', async () => {
    const { core, edits, freed, refreshed } = fakeCore({ assembly: false });
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await expect(engine.applyEdit(handle, { kind: 'rotate', from: 0, count: 1, degrees: 90 })).rejects.toMatchObject({
      name: 'EditNotPermitted',
    });
    expect(freed).toContain('edit:rotate');
    expect(refreshed).toEqual([]);
  });

  it('takes an edit back when the preview cannot show it, so the pages never disagree with the pixels', async () => {
    const { core, edits } = fakeCore({ pages: 2, failRefresh: true });
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await expect(engine.applyEdit(handle, { kind: 'remove', from: 0, count: 1 })).rejects.toThrow('preview failed');
    expect(core.pageCount(handle)).toBe(2);
    expect(core.canUndo(handle)).toBe(false);
  });

  it('prepares an import source, copying what the core reports and freeing it', async () => {
    const { core, edits, freed, prepared } = fakeCore();
    const engine = documentEngine(core, geometry, edits);
    expect(await engine.prepareImport(new Uint8Array([1, 2]))).toEqual({
      id: 1,
      pageCount: 2,
      warnings: ['a link was dropped'],
    });
    expect(freed).toEqual(['prepared']);
    await engine.discardImport(1);
    expect(prepared.size).toBe(0);
  });

  it('passes a password failure of an import source through by name', async () => {
    const { core, edits } = fakeCore();
    const engine = documentEngine(core, geometry, edits);
    await expect(engine.prepareImport(new Uint8Array([0xee]))).rejects.toMatchObject({ name: 'PasswordRequired' });
    await expect(engine.prepareImport(new Uint8Array([0xee]), 'nope')).rejects.toMatchObject({ name: 'WrongPassword' });
    expect((await engine.prepareImport(new Uint8Array([0xee]), 'secret')).pageCount).toBe(1);
  });

  it('imports a pick as one step, refreshes the preview and answers with the blocks and source ids', async () => {
    const { core, edits, freed, refreshed } = fakeCore({ pages: 2 });
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    const a = await engine.prepareImport(new Uint8Array([1, 1]));
    const b = await engine.prepareImport(new Uint8Array([1]));

    const imported = await engine.importPrepared(handle, [a.id, b.id], 1);
    expect(imported.sourceIds).toEqual([1, 2]);
    expect(imported).toMatchObject({ canUndo: true, canRedo: false });
    expect(imported.info.pageIds).toEqual([100, 200, 201, 202, 101]);
    expect(imported.info.blocks).toEqual([
      { source: { kind: 'base' }, part: null, start: 0, count: 1 },
      { source: { kind: 'imported', id: 1 }, part: null, start: 1, count: 2 },
      { source: { kind: 'imported', id: 2 }, part: null, start: 3, count: 1 },
      { source: { kind: 'base' }, part: null, start: 4, count: 1 },
    ]);
    expect(refreshed).toEqual([handle]);
    expect(freed).toContain('imported');

    const undone = await engine.undo(handle);
    expect(undone.info.pageIds).toEqual([100, 101]);
  });

  it('reports why a document refuses an import, and passes the refusal through', async () => {
    const { core, edits, refreshed } = fakeCore({ assembly: false });
    const engine = documentEngine(core, geometry, edits);
    const { handle, info } = await engine.open(new Uint8Array([10]));
    expect(info.importRefusal).toBe('no assembly');
    const source = await engine.prepareImport(new Uint8Array([1]));
    await expect(engine.importPrepared(handle, [source.id], 0)).rejects.toMatchObject({ name: 'EditNotPermitted' });
    expect(refreshed).toEqual([]);
  });

  it('takes an import back when the preview cannot show it', async () => {
    const { core, edits } = fakeCore({ pages: 1, failRefresh: true });
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    const source = await engine.prepareImport(new Uint8Array([1]));
    await expect(engine.importPrepared(handle, [source.id], 1)).rejects.toThrow('preview failed');
    expect(core.pageCount(handle)).toBe(1);
  });

  it('refuses an edit of a closed document', async () => {
    const { core, edits } = fakeCore();
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.close(handle);
    await expect(engine.undo(handle)).rejects.toMatchObject({ name: 'DocumentNotFound' });
  });
});
