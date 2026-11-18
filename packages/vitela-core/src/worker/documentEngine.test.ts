import { describe, expect, it } from 'vitest';
import { documentEngine, type CoreEdits, type CoreEngine, type CoreFunctions } from './documentEngine.js';
import type { CoreAnnotation, CoreFreeTextLines } from './annotations.js';
import type { CoreGeometry, CorePageCharacters } from './pageText.js';

/** A fake core layout: one line per word, at 12 pt. */
function fakeLayout(contents: string, free: () => void = () => {}): CoreFreeTextLines {
  const texts = contents.split(' ').filter(Boolean);
  return {
    fontSizePt: 12,
    texts,
    xs: new Float64Array(texts.map(() => 2)),
    baselines: new Float64Array(texts.map((_, i) => 11 + i * 14)),
    overflow: false,
    free,
  };
}

/** The core's free functions besides placement. */
const annotationFunctions = {
  freeTextLayout: (contents: string) => {
    if (/[^\x20-\x7e]/.test(contents)) {
      const error = new Error(`'${contents}' cannot be shown in Helvetica (WinAnsi)`);
      error.name = 'EncodingGap';
      throw error;
    }
    return fakeLayout(contents);
  },
  // A 2:1 image: 144 x 72 pt with its top-left at the anchor.
  stampPlacement: (_image: Uint8Array, x: number, y: number) => new Float64Array([x, y - 72, 144, 72]),
};

/** Placement that only marks its output, so a test sees it was applied. */
const geometry: CoreFunctions = {
  placeRects: (rects) => rects.map((value) => value + 1000),
  pointToPdf: (x, y) => new Float32Array([x, y]),
  ...annotationFunctions,
};

/**
 * Placement that flips y like an upright page does (PDF space has its origin
 * bottom-left, the drawn page top-left): what annotation geometry is tested
 * against. Turns are the core's and are tested against the real one.
 */
const flipping: CoreGeometry = {
  placeRects: (rects, _width, height) => {
    const placed = new Float32Array(rects.length);
    for (let i = 0; i + 3 < rects.length; i += 4) {
      placed.set([rects[i]!, height - rects[i + 1]! - rects[i + 3]!, rects[i + 2]!, rects[i + 3]!], i);
    }
    return placed;
  },
  pointToPdf: (x, y, _width, height) => new Float32Array([x, height - y]),
};
const upright: CoreFunctions = { ...flipping, ...annotationFunctions };

/** One fake page: its identity, size and the PDF it came from (0: the opened one). */
interface FakePage {
  readonly id: number;
  readonly widthPt: number;
  readonly rotationDegrees: number;
  readonly source?: number;
}

/** One fake annotation, on the page with id `pageId`, its geometry as the core got it. */
interface FakeAnnotation {
  readonly id: number;
  readonly pageId: number;
  readonly kind: string;
  readonly rect: number[];
  readonly points: number[];
  readonly color: number | undefined;
  readonly contents: string;
}

/** What a fake `EditCommand` carries; the real one is an opaque wasm object. */
type FakeEdit =
  | { kind: 'move'; from: number; count: number; to: number; free(): void }
  | { kind: 'rotate'; from: number; count: number; delta: number; free(): void }
  | { kind: 'remove'; from: number; count: number; free(): void }
  | { kind: 'annotation'; op: string; args: unknown[]; free(): void };

type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;

/** The document as one step left it, and whether that step shows in the pixels. */
interface FakeState {
  pages: FakePage[];
  annotations: FakeAnnotation[];
}
interface FakeStep {
  before: FakeState;
  /** Set when the preview shows the step: which state of the history it is. */
  mark: number | undefined;
}

function named(name: string, message: string): Error {
  const error = new Error(message);
  error.name = name;
  return error;
}

/** A core that keeps documents by id like core/pdf-wasm does. */
function fakeCore(
  options: {
    failDimensions?: boolean;
    restricted?: boolean;
    pages?: number;
    assembly?: boolean;
    annotate?: boolean;
    failRefresh?: boolean;
    /** Emulates a core whose stale flag only ever clears on a refresh. */
    stickyStale?: boolean;
  } = {},
) {
  interface Doc extends FakeState {
    marker: number;
    undo: FakeStep[];
    redo: FakeStep[];
    /** The mark of the state the preview shows (0: as opened). */
    shown: number;
    nextMark: number;
    sticky: boolean;
    nextAnnotation: number;
  }
  const open = new Map<number, Doc>();
  const closed: number[] = [];
  const freed: string[] = [];
  const saves: [number, boolean][] = [];
  const refreshed: number[] = [];
  /** Every `applyEdit` the core received, as the fake command it was. */
  const applied: FakeEdit[] = [];
  /** Prepared import sources: id -> page count. */
  const prepared = new Map<number, number>();
  let nextId = 0;
  let nextPrepared = 1;
  let nextSource = 1;
  let nextPageId = 200;
  const known = (doc: number) => {
    const entry = open.get(doc);
    if (entry === undefined) throw named('DocumentNotFound', `document ${doc} is not open`);
    return entry;
  };
  const readable = (doc: number) => {
    known(doc);
    if (options.restricted) throw named('TextExtractionDenied', 'text extraction is not permitted');
  };
  /** The mark of the newest shown step still applied, as the core's `PreviewMarks`. */
  const current = (entry: Doc) => [...entry.undo].reverse().find((step) => step.mark !== undefined)?.mark ?? 0;
  /** Records a step the way the core's log does. */
  const record = (entry: Doc, next: FakeState, pixels: boolean) => {
    const mark = pixels ? ++entry.nextMark : undefined;
    entry.undo.push({ before: { pages: entry.pages, annotations: entry.annotations }, mark });
    entry.redo = [];
    entry.pages = next.pages;
    entry.annotations = next.annotations;
    entry.sticky ||= pixels;
  };
  const step = (entry: Doc, from: FakeStep[], to: FakeStep[]) => {
    const taken = from.pop();
    if (!taken) return false;
    to.push({ before: { pages: entry.pages, annotations: entry.annotations }, mark: taken.mark });
    entry.pages = taken.before.pages;
    entry.annotations = taken.before.annotations;
    entry.sticky ||= taken.mark !== undefined;
    return true;
  };
  const command = (edit: DistributiveOmit<FakeEdit, 'free'>): FakeEdit =>
    ({ ...edit, free: () => freed.push(`edit:${edit.kind === 'annotation' ? edit.op : edit.kind}`) }) as FakeEdit;
  const annotation =
    (op: string) =>
    (...args: unknown[]) =>
      command({ kind: 'annotation', op, args });
  const edits: CoreEdits = {
    movePages: (from, count, to) => command({ kind: 'move', from, count, to }),
    rotatePages: (from, count, delta) => command({ kind: 'rotate', from, count, delta }),
    removePages: (from, count) => command({ kind: 'remove', from, count }),
    addHighlight: annotation('addHighlight'),
    addUnderline: annotation('addUnderline'),
    addStrikeout: annotation('addStrikeout'),
    addShape: annotation('addShape'),
    addInk: annotation('addInk'),
    addNote: annotation('addNote'),
    addFreeText: annotation('addFreeText'),
    addStamp: annotation('addStamp'),
    moveAnnotation: annotation('moveAnnotation'),
    resizeAnnotation: annotation('resizeAnnotation'),
    recolorAnnotation: annotation('recolorAnnotation'),
    retextAnnotation: annotation('retextAnnotation'),
    removeAnnotation: annotation('removeAnnotation'),
  };
  /** What a fake annotation edit leaves, or the error the core would give. */
  const annotate = (entry: Doc, op: string, args: unknown[]): { next: FakeAnnotation[]; added?: number } => {
    if (options.annotate === false) throw named('EditNotPermitted', 'annotation editing is not permitted');
    const kinds: Record<string, string> = {
      addHighlight: 'highlight',
      addUnderline: 'underline',
      addStrikeout: 'strikeout',
      addShape: 'shape',
      addInk: 'ink',
      addNote: 'note',
      addFreeText: 'freeText',
      addStamp: 'stamp',
    };
    const kind = kinds[op];
    if (kind) {
      const page = args[0] as number;
      const pageId = entry.pages[page]?.id;
      if (pageId === undefined) throw named('PageIndexOutOfBounds', `page index ${page} is out of bounds`);
      const id = entry.nextAnnotation++;
      const numbers = (from: number) => (args.slice(from, from + 4) as number[]).map(Number);
      const added: FakeAnnotation =
        kind === 'ink'
          ? { id, pageId, kind, rect: [], points: Array.from(args[1] as Float64Array), color: args[2] as number, contents: '' }
          : kind === 'stamp'
            ? { id, pageId, kind, rect: numbers(2), points: [], color: undefined, contents: '' }
            : kind === 'note' || kind === 'freeText'
              ? { id, pageId, kind, rect: numbers(1), points: [], color: undefined, contents: args[5] as string }
              : { id, pageId, kind, rect: numbers(1), points: [], color: args[5] as number, contents: '' };
      return { next: [...entry.annotations, added], added: id };
    }
    const id = args[0] as number;
    const target = entry.annotations.find((a) => a.id === id);
    if (!target) throw named('AnnotationNotFound', `no annotation with id ${id}`);
    const others = entry.annotations.filter((a) => a !== target);
    const replace = (changed: FakeAnnotation) => ({ next: entry.annotations.map((a) => (a === target ? changed : a)) });
    switch (op) {
      case 'removeAnnotation':
        return { next: others };
      case 'moveAnnotation': {
        const [dx, dy] = args.slice(1) as number[];
        const [x, y, w, h] = target.rect;
        return replace({ ...target, rect: [x! + dx!, y! + dy!, w!, h!] });
      }
      case 'resizeAnnotation':
        return replace({ ...target, rect: (args.slice(1) as number[]).map(Number) });
      case 'recolorAnnotation':
        return replace({ ...target, color: args[1] as number });
      default:
        return replace({ ...target, contents: args[1] as string });
    }
  };
  const core: CoreEngine = {
    open: (bytes) => {
      const id = nextId++;
      const pages = Array.from({ length: options.pages ?? 1 }, (_, index) => ({
        id: 100 + index,
        widthPt: 612,
        rotationDegrees: 0,
      }));
      open.set(id, {
        marker: bytes[0]!,
        pages,
        annotations: [],
        undo: [],
        redo: [],
        shown: 0,
        nextMark: 0,
        sticky: false,
        nextAnnotation: 0,
      });
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
    annotationEditingAllowed: (doc) => (known(doc), options.annotate ?? true),
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
      const edit = raw as FakeEdit;
      applied.push(edit);
      if (edit.kind === 'annotation') {
        const { next, added } = annotate(entry, edit.op, edit.args);
        record(entry, { pages: entry.pages, annotations: next }, false);
        return added;
      }
      if (options.assembly === false) throw named('EditNotPermitted', 'the document does not permit page assembly');
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
      // Removing a page takes what is on it.
      const kept = new Set(pages.map((page) => page.id));
      record(entry, { pages, annotations: entry.annotations.filter((a) => kept.has(a.pageId)) }, true);
      return undefined;
    },
    undo: (doc) => {
      const entry = known(doc);
      return step(entry, entry.undo, entry.redo);
    },
    redo: (doc) => {
      const entry = known(doc);
      return step(entry, entry.redo, entry.undo);
    },
    prepareImport: (bytes, password) => {
      // A source whose first byte is 0xee is locked under "secret".
      if (bytes[0] === 0xee && password !== 'secret') {
        throw named(password === undefined ? 'PasswordRequired' : 'WrongPassword', 'password needed');
      }
      const id = nextPrepared++;
      prepared.set(id, bytes.length);
      return { id, pageCount: bytes.length, warnings: ['a link was dropped'], free: () => freed.push('prepared') };
    },
    discardPrepared: (id) => prepared.delete(id),
    importPrepared: (doc, ids, index) => {
      const entry = known(doc);
      if (options.assembly === false) throw named('EditNotPermitted', 'the document does not permit page assembly');
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
      record(entry, { pages, annotations: entry.annotations }, true);
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
      const entry = known(doc);
      if (options.failRefresh) throw new Error('preview failed');
      refreshed.push(doc);
      entry.shown = current(entry);
      entry.sticky = false;
    },
    previewStale: (doc) => {
      const entry = known(doc);
      return options.stickyStale ? entry.sticky : current(entry) !== entry.shown;
    },
    annotations: (doc) => {
      const entry = known(doc);
      return entry.annotations.map(
        (a): CoreAnnotation => ({
          id: a.id,
          page: entry.pages.findIndex((page) => page.id === a.pageId),
          kind: a.kind,
          rect: new Float64Array(a.rect),
          points: new Float64Array(a.points),
          color: a.color,
          contents: a.contents,
          layout: a.kind === 'freeText' ? fakeLayout(a.contents, () => freed.push('layout')) : undefined,
          free: () => freed.push(`annotation:${a.id}`),
        }),
      );
    },
    comments: (doc) => {
      const entry = known(doc);
      const fromFile = {
        page: 0,
        rect: new Float64Array([40, 600, 20, 20]),
        contents: 'Check this',
        author: 'Ana',
        date: 'D:20261008',
        annotationId: undefined,
        free: () => freed.push('comment'),
      };
      const notes = entry.annotations
        .filter((a) => a.kind === 'note')
        .map((a) => ({
          page: entry.pages.findIndex((page) => page.id === a.pageId),
          rect: new Float64Array(a.rect),
          contents: a.contents,
          // wasm-bindgen's `None`.
          author: undefined,
          date: undefined,
          annotationId: a.id,
          free: () => freed.push('comment'),
        }));
      return [fromFile, ...notes];
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
  return { core, edits, closed, freed, saves, refreshed, prepared, applied };
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
      annotationEditingAllowed: true,
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

  it('survives two refresh failures in a row, each taking back only its own step', async () => {
    const options = { pages: 3, failRefresh: true };
    const { core, edits, refreshed } = fakeCore(options);
    const engine = documentEngine(core, geometry, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await expect(engine.applyEdit(handle, { kind: 'remove', from: 0, count: 1 })).rejects.toThrow('preview failed');
    await expect(engine.applyEdit(handle, { kind: 'move', from: 0, count: 1, to: 2 })).rejects.toThrow('preview failed');
    expect(core.pageIds(handle)).toEqual(new Uint32Array([100, 101, 102]));
    expect(core.canUndo(handle)).toBe(false);
    // Once the preview works again, nothing is left over for it to show.
    options.failRefresh = false;
    expect(core.previewStale(handle)).toBe(false);
    const turned = await engine.applyEdit(handle, { kind: 'rotate', from: 0, count: 1, degrees: 90 });
    expect(turned).toMatchObject({ canUndo: true, canRedo: false });
    expect(refreshed).toEqual([handle]);
  });

  it('undoes nothing, and refreshes nothing, when there is nothing to undo after a failed refresh', async () => {
    const options = { pages: 2, failRefresh: false };
    const { core, edits, refreshed } = fakeCore(options);
    const engine = documentEngine(core, upright, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    options.failRefresh = true;
    await expect(engine.applyEdit(handle, { kind: 'remove', from: 0, count: 1 })).rejects.toThrow('preview failed');

    const undone = await engine.undo(handle);
    expect(undone).toMatchObject({ canUndo: false });
    expect(undone.info.pageIds).toEqual([100, 101]);
    expect(refreshed).toEqual([]);
  });

  it('keeps an annotation edit made after a failed refresh: it owes the preview nothing', async () => {
    const options = { pages: 2, failRefresh: false };
    const { core, edits, refreshed } = fakeCore(options);
    const engine = documentEngine(core, upright, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    const box = { left: 72, top: 72, width: 120, height: 30 };
    await engine.applyEdit(handle, { kind: 'addShape', page: 1, rect: box, color: { r: 0, g: 0, b: 0 } });
    options.failRefresh = true;
    await expect(engine.applyEdit(handle, { kind: 'remove', from: 0, count: 1 })).rejects.toThrow('preview failed');

    const added = await engine.applyEdit(handle, { kind: 'addShape', page: 0, rect: box, color: { r: 0, g: 0, b: 0 } });
    expect(added.addedAnnotationId).toBe(1);
    expect(added.annotations.map((a) => a.id)).toEqual([0, 1]);
    expect(added.info.pageIds).toEqual([100, 101]);
    // Undo takes back the user's own steps, newest first, never the reverted page step.
    expect((await engine.undo(handle)).annotations.map((a) => a.id)).toEqual([0]);
    const first = await engine.undo(handle);
    expect(first).toMatchObject({ canUndo: false, annotations: [] });
    expect(first.info.pageIds).toEqual([100, 101]);
    expect(refreshed).toEqual([]);
  });

  it('brings a preview left behind up to date before the step, and never reverts a step it did not take', async () => {
    // A core that over-reports staleness: the worker must not blame the
    // next request for it.
    const options = { pages: 2, failRefresh: true, stickyStale: true };
    const { core, edits, refreshed } = fakeCore(options);
    const engine = documentEngine(core, upright, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await expect(engine.applyEdit(handle, { kind: 'remove', from: 0, count: 1 })).rejects.toThrow('preview failed');
    expect(core.previewStale(handle)).toBe(true);

    // Undo still runs while the preview is behind (it may be the way out),
    // but with nothing to undo it takes nothing back: the page step reverted
    // above is not redone as a "revert".
    const undone = await engine.undo(handle);
    expect(undone).toMatchObject({ canUndo: false });
    expect(core.pageIds(handle)).toEqual(new Uint32Array([100, 101]));

    // Any other edit is refused before it records anything.
    const box = { left: 72, top: 72, width: 120, height: 30 };
    await expect(
      engine.applyEdit(handle, { kind: 'addShape', page: 0, rect: box, color: { r: 0, g: 0, b: 0 } }),
    ).rejects.toThrow('preview failed');
    expect(core.canUndo(handle)).toBe(false);

    options.failRefresh = false;
    const added = await engine.applyEdit(handle, { kind: 'addShape', page: 0, rect: box, color: { r: 0, g: 0, b: 0 } });
    expect(added).toMatchObject({ canUndo: true, addedAnnotationId: 0 });
    expect(added.info.pageIds).toEqual([100, 101]);
    // The catch-up, before the step; the shape itself needs none.
    expect(refreshed).toEqual([handle]);
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

describe('documentEngine annotations', () => {
  const yellow = { r: 255, g: 255, b: 0 };
  const box = { left: 72, top: 72, width: 120, height: 30 };
  /** `box` in the upright fake's PDF space: y flipped on a 792 pt page. */
  const pdfBox = [72, 690, 120, 30];

  it('adds markup on the page as drawn as one step, with no refresh, and answers with the snapshot and its id', async () => {
    const { core, edits, applied, freed, refreshed } = fakeCore({ pages: 2 });
    const engine = documentEngine(core, upright, edits);
    const { handle, info } = await engine.open(new Uint8Array([10]));
    expect(info.annotationEditingAllowed).toBe(true);

    const added = await engine.applyEdit(handle, { kind: 'addMarkup', page: 1, markup: 'highlight', rect: box, color: yellow });

    expect(applied.at(-1)).toMatchObject({ op: 'addHighlight', args: [1, ...pdfBox, 0xffff00] });
    expect(added).toMatchObject({ canUndo: true, canRedo: false, addedAnnotationId: 0 });
    expect(added.annotations).toEqual([{ kind: 'highlight', id: 0, pageIndex: 1, rect: box, color: yellow }]);
    // The preview leaves annotations out: nothing to reopen.
    expect(refreshed).toEqual([]);
    expect(freed).toEqual(['edit:addHighlight', 'annotation:0']);
  });

  it('sends each markup kind to its own constructor', async () => {
    const { core, edits, applied } = fakeCore();
    const engine = documentEngine(core, upright, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.applyEdit(handle, { kind: 'addMarkup', page: 0, markup: 'underline', rect: box, color: yellow });
    await engine.applyEdit(handle, { kind: 'addMarkup', page: 0, markup: 'strikeout', rect: box, color: yellow });
    expect(applied.map((edit) => edit.kind === 'annotation' && edit.op)).toEqual(['addUnderline', 'addStrikeout']);
    const { annotations } = await engine.undo(handle);
    expect(annotations.map((a) => a.kind)).toEqual(['underline']);
  });

  it('refreshes across a mixed history only on the page steps, and the snapshot follows the page', async () => {
    const { core, edits, refreshed } = fakeCore({ pages: 2 });
    const engine = documentEngine(core, upright, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.applyEdit(handle, { kind: 'addMarkup', page: 0, markup: 'highlight', rect: box, color: yellow });

    const moved = await engine.applyEdit(handle, { kind: 'move', from: 0, count: 1, to: 1 });
    expect(refreshed).toHaveLength(1);
    expect(moved.addedAnnotationId).toBeNull();
    expect(moved.annotations[0]).toMatchObject({ id: 0, pageIndex: 1, rect: box });

    const undoneMove = await engine.undo(handle);
    expect(refreshed).toHaveLength(2);
    expect(undoneMove.annotations[0]).toMatchObject({ pageIndex: 0 });

    const undoneMarkup = await engine.undo(handle);
    expect(undoneMarkup.annotations).toEqual([]);
    expect(undoneMarkup.addedAnnotationId).toBeNull();
    await engine.redo(handle);
    expect(refreshed).toHaveLength(2);
    const redoneMove = await engine.redo(handle);
    expect(refreshed).toHaveLength(3);
    expect(redoneMove.annotations[0]).toMatchObject({ pageIndex: 1 });
  });

  it('converts and decodes every kind, on the page as drawn', async () => {
    const { core, edits, applied, freed } = fakeCore();
    const engine = documentEngine(core, upright, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    const image = new Uint8Array([137, 80]);
    const steps = [
      { kind: 'addShape', page: 0, rect: box, color: yellow },
      { kind: 'addInk', page: 0, points: [{ x: 10, y: 20 }, { x: 30, y: 50 }], color: yellow },
      { kind: 'addNote', page: 0, rect: box, contents: 'Call back' },
      { kind: 'addFreeText', page: 0, rect: box, contents: 'Hello box' },
      { kind: 'addStamp', page: 0, rect: box, image },
    ] as const;
    let last;
    for (const step of steps) last = await engine.applyEdit(handle, step);

    expect(applied.map((edit) => edit.kind === 'annotation' && edit.args)).toEqual([
      [0, ...pdfBox, 0xffff00],
      [0, new Float64Array([10, 772, 30, 742]), 0xffff00],
      [0, ...pdfBox, 'Call back'],
      [0, ...pdfBox, 'Hello box'],
      [0, image, ...pdfBox],
    ]);
    expect(last!.annotations).toEqual([
      { kind: 'shape', id: 0, pageIndex: 0, rect: box, color: yellow },
      { kind: 'ink', id: 1, pageIndex: 0, points: [{ x: 10, y: 20 }, { x: 30, y: 50 }], color: yellow },
      { kind: 'note', id: 2, pageIndex: 0, rect: box, contents: 'Call back' },
      {
        kind: 'freeText',
        id: 3,
        pageIndex: 0,
        rect: box,
        contents: 'Hello box',
        layout: {
          fontSizePt: 12,
          lines: [
            { text: 'Hello', xPt: 2, baselineFromTopPt: 11 },
            { text: 'box', xPt: 2, baselineFromTopPt: 25 },
          ],
          overflow: false,
        },
      },
      { kind: 'stamp', id: 4, pageIndex: 0, rect: box },
    ]);
    expect(freed).toContain('layout');
    expect(last!.addedAnnotationId).toBe(4);
  });

  it('moves and resizes on the page as drawn', async () => {
    const { core, edits, applied } = fakeCore();
    const engine = documentEngine(core, upright, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.applyEdit(handle, { kind: 'addShape', page: 0, rect: box, color: yellow });

    const moved = await engine.applyEdit(handle, { kind: 'moveAnnotation', id: 0, dx: 5, dy: 10 });
    // Down the drawn page is down the PDF y axis.
    expect(applied.at(-1)).toMatchObject({ op: 'moveAnnotation', args: [0, 5, -10] });
    expect(moved.annotations[0]).toMatchObject({ rect: { left: 77, top: 82, width: 120, height: 30 } });

    const bigger = { left: 10, top: 20, width: 200, height: 100 };
    const resized = await engine.applyEdit(handle, { kind: 'resizeAnnotation', id: 0, rect: bigger });
    expect(applied.at(-1)).toMatchObject({ op: 'resizeAnnotation', args: [0, 10, 672, 200, 100] });
    expect(resized.annotations[0]).toMatchObject({ rect: bigger });
  });

  it('passes recolour, retext and removal through by id', async () => {
    const { core, edits, applied } = fakeCore();
    const engine = documentEngine(core, upright, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.applyEdit(handle, { kind: 'addFreeText', page: 0, rect: box, contents: 'old' });
    await engine.applyEdit(handle, { kind: 'recolorAnnotation', id: 0, color: { r: 300, g: -4, b: 127.6 } });
    await engine.applyEdit(handle, { kind: 'retextAnnotation', id: 0, contents: 'new' });
    const removed = await engine.applyEdit(handle, { kind: 'removeAnnotation', id: 0 });
    expect(applied.slice(1).map((edit) => edit.kind === 'annotation' && [edit.op, ...edit.args])).toEqual([
      ['recolorAnnotation', 0, 0xff0080],
      ['retextAnnotation', 0, 'new'],
      ['removeAnnotation', 0],
    ]);
    expect(removed.annotations).toEqual([]);
  });

  it("leaves the order of refusals to the core: the permission before the page or the id", async () => {
    const { core, edits, freed, applied, refreshed } = fakeCore({ annotate: false });
    const engine = documentEngine(core, upright, edits);
    const { handle, info } = await engine.open(new Uint8Array([10]));
    expect(info.annotationEditingAllowed).toBe(false);
    await expect(
      engine.applyEdit(handle, { kind: 'addMarkup', page: 9, markup: 'highlight', rect: box, color: yellow }),
    ).rejects.toMatchObject({ name: 'EditNotPermitted' });
    expect(freed).toContain('edit:addHighlight');
    expect(applied).toHaveLength(1);
    expect(refreshed).toEqual([]);
  });

  it('sends a page past the end or an unknown id as given, for the core to name', async () => {
    const { core, edits, applied } = fakeCore();
    const engine = documentEngine(core, upright, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await expect(
      engine.applyEdit(handle, { kind: 'addMarkup', page: 3, markup: 'highlight', rect: box, color: yellow }),
    ).rejects.toMatchObject({ name: 'PageIndexOutOfBounds' });
    expect(applied.at(-1)).toMatchObject({ args: [3, 72, 72, 120, 30, 0xffff00] });
    await expect(engine.applyEdit(handle, { kind: 'removeAnnotation', id: 9 })).rejects.toMatchObject({
      name: 'AnnotationNotFound',
    });
    expect(applied.at(-1)).toMatchObject({ args: [9] });
  });

  it('refuses to move or resize an annotation the snapshot does not hold: its drawn geometry has no page', async () => {
    const { core, edits, applied } = fakeCore({ pages: 2 });
    const engine = documentEngine(core, upright, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.applyEdit(handle, { kind: 'addShape', page: 1, rect: box, color: yellow });
    // Its page goes; the core still holds the annotation for an undo.
    const removed = await engine.applyEdit(handle, { kind: 'remove', from: 1, count: 1 });
    expect(removed.annotations).toEqual([]);
    const sent = applied.length;

    await expect(engine.applyEdit(handle, { kind: 'moveAnnotation', id: 0, dx: 5, dy: 10 })).rejects.toMatchObject({
      name: 'AnnotationNotFound',
    });
    await expect(engine.applyEdit(handle, { kind: 'resizeAnnotation', id: 0, rect: box })).rejects.toMatchObject({
      name: 'AnnotationNotFound',
    });
    await expect(engine.applyEdit(handle, { kind: 'moveAnnotation', id: 9, dx: 5, dy: 10 })).rejects.toMatchObject({
      name: 'AnnotationNotFound',
    });
    // Nothing reached the core, so nothing was recorded.
    expect(applied).toHaveLength(sent);
    const undone = await engine.undo(handle);
    expect(undone.info.pageIds).toEqual([100, 101]);
    expect(undone.annotations[0]).toMatchObject({ id: 0, rect: box });
  });

  it('leaves a move of an annotation it cannot place to the core on a document that refuses annotation edits', async () => {
    const { core, edits, applied } = fakeCore({ annotate: false });
    const engine = documentEngine(core, upright, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await expect(engine.applyEdit(handle, { kind: 'moveAnnotation', id: 9, dx: 3, dy: 4 })).rejects.toMatchObject({
      name: 'EditNotPermitted',
    });
    await expect(engine.applyEdit(handle, { kind: 'resizeAnnotation', id: 9, rect: box })).rejects.toMatchObject({
      name: 'EditNotPermitted',
    });
    expect(applied).toHaveLength(2);
  });

  it('lists the comments on the page as drawn, an absent author or id as null', async () => {
    const { core, edits, freed } = fakeCore();
    const engine = documentEngine(core, upright, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.applyEdit(handle, { kind: 'addNote', page: 0, rect: box, contents: 'Mine' });

    expect(await engine.comments(handle)).toEqual([
      {
        pageIndex: 0,
        rect: { left: 40, top: 172, width: 20, height: 20 },
        contents: 'Check this',
        author: 'Ana',
        date: 'D:20261008',
        annotationId: null,
      },
      { pageIndex: 0, rect: box, contents: 'Mine', author: null, date: null, annotationId: 0 },
    ]);
    expect(freed.filter((entry) => entry === 'comment')).toHaveLength(2);
  });

  it('answers the snapshot on request, and refuses a closed document', async () => {
    const { core, edits } = fakeCore();
    const engine = documentEngine(core, upright, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    expect(await engine.annotations(handle)).toEqual([]);
    await engine.applyEdit(handle, { kind: 'addShape', page: 0, rect: box, color: yellow });
    expect(await engine.annotations(handle)).toHaveLength(1);
    await engine.close(handle);
    await expect(engine.annotations(handle)).rejects.toMatchObject({ name: 'DocumentNotFound' });
    await expect(engine.comments(handle)).rejects.toMatchObject({ name: 'DocumentNotFound' });
  });

  it('lays out free text, and passes EncodingGap through', async () => {
    const { core, edits } = fakeCore();
    const engine = documentEngine(core, upright, edits);
    expect(await engine.freeTextLayout('Hi there', 100, 40)).toEqual({
      fontSizePt: 12,
      lines: [
        { text: 'Hi', xPt: 2, baselineFromTopPt: 11 },
        { text: 'there', xPt: 2, baselineFromTopPt: 25 },
      ],
      overflow: false,
    });
    await expect(engine.freeTextLayout('日', 100, 40)).rejects.toMatchObject({ name: 'EncodingGap' });
  });

  it("asks the core to place a stamp at the anchor in PDF space, and draws the core's box", async () => {
    const { core, edits } = fakeCore({ pages: 2 });
    const anchors: [number, number][] = [];
    const functions: CoreFunctions = {
      ...upright,
      stampPlacement: (image, x, y) => (anchors.push([x, y]), annotationFunctions.stampPlacement(image, x, y)),
    };
    const engine = documentEngine(core, functions, edits);
    const { handle } = await engine.open(new Uint8Array([10]));
    const image = new Uint8Array([1]);
    // The core's box hangs below the anchor in PDF space: drawn, below and right of it.
    expect(await engine.stampPlacement(handle, 0, image, { x: 50, y: 60 })).toEqual({
      left: 50,
      top: 60,
      width: 144,
      height: 72,
    });
    expect(anchors).toEqual([[50, 732]]);
    await expect(engine.stampPlacement(handle, 2, image, { x: 0, y: 0 })).rejects.toMatchObject({
      name: 'PageIndexOutOfBounds',
    });
  });
});
