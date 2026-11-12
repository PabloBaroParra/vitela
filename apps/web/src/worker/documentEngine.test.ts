import { describe, expect, it } from 'vitest';
import { documentEngine, type CoreEngine } from './documentEngine';

/** A core that keeps documents by id like core/pdf-wasm does. */
function fakeCore(options: { failDimensions?: boolean } = {}) {
  const open = new Map<number, number>();
  const closed: number[] = [];
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
  };
  return { core, closed };
}

describe('documentEngine', () => {
  it('returns a handle per document and keeps an earlier one open', async () => {
    const { core, closed } = fakeCore();
    const engine = documentEngine(core);
    const a = await engine.open(new Uint8Array([10]));
    const b = await engine.open(new Uint8Array([20]));
    expect(a.handle).not.toBe(b.handle);
    expect(a.info).toEqual({ pageCount: 1, pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }] });
    expect(closed).toEqual([]);
    // Each render reaches the document its handle names, not the newest one.
    expect((await engine.renderPage(a.handle, 0, 72)).rgba[0]).toBe(10);
    expect((await engine.renderPage(b.handle, 0, 72)).rgba[0]).toBe(20);
  });

  it('refuses to render a document once it was closed', async () => {
    const { core } = fakeCore();
    const engine = documentEngine(core);
    const { handle } = await engine.open(new Uint8Array([10]));
    await engine.close(handle);
    await expect(engine.renderPage(handle, 0, 72)).rejects.toMatchObject({ name: 'DocumentNotFound' });
  });

  it('closes a document it opened but could not describe', async () => {
    const { core, closed } = fakeCore({ failDimensions: true });
    const engine = documentEngine(core);
    await expect(engine.open(new Uint8Array([10]))).rejects.toThrow('dimensions failed');
    expect(closed).toEqual([0]);
  });
});
