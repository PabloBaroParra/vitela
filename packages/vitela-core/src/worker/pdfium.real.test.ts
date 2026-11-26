// @vitest-environment node
//
// The init alias against the REAL pinned pdfium and the real core module,
// run under Node (Emscripten's glue supports it). Needs `npm run build:wasm`
// first; CI runs it before Vitest.

import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';
import { documentEngine } from './documentEngine.js';
import { coreEdits, coreFunctions } from './engine.js';
import { instantiatePdfium, type PdfiumModule } from './pdfium.js';
import { FORM_FIELD_NOT_FOUND } from '../core/errors.js';
import type { FormField } from '../core/types.js';
import * as core from '../generated/engine/pdf_wasm.js';

const here = dirname(fileURLToPath(import.meta.url));
const generated = resolve(here, '../generated');
const fixture = resolve(here, '../../../../tests/fixtures/forms/reportlab_acroform.pdf');

describe('real pdfium chromium/7763 + core/pdf-wasm', () => {
  let pdfium: PdfiumModule;
  let coreExports: unknown;
  const glueSource = readFileSync(resolve(generated, 'pdfium/pdfium.js'), 'utf8');

  beforeAll(async () => {
    // Under Node, Emscripten's glue reads the .wasm with require('fs').
    const scope = globalThis as Record<string, unknown>;
    scope.require = createRequire(import.meta.url);
    scope.__dirname = resolve(generated, 'pdfium');
    pdfium = await instantiatePdfium(glueSource, resolve(generated, 'pdfium/pdfium.wasm'));
    coreExports = core.initSync({ module: readFileSync(resolve(generated, 'engine/pdf_wasm_bg.wasm')) });
  });

  it('exports only _FPDF_InitLibrary, none of the names pdfium-render looks up', () => {
    // pdfium-render 0.9.5 calls `PDF_InitLibrary` when `FPDF_InitLibrary`
    // exists, else `PDFium_Init`. If this build ever starts exporting one of
    // those, the alias in pdfium.ts is obsolete (or shadowing something).
    expect(typeof pdfium._FPDF_InitLibrary).toBe('function');
    expect(pdfium.FPDF_InitLibrary).toBeUndefined();
    expect(pdfium._PDF_InitLibrary).toBeUndefined();
    expect(glueSource).not.toMatch(/\b_?(PDFium_Init|PDF_InitLibrary)\b/);
    // instantiatePdfium applied the alias.
    expect(pdfium._PDFium_Init).toBe(pdfium._FPDF_InitLibrary);
  });

  it('opens a repo fixture and renders non-blank pixels through the alias', () => {
    expect(core.initialize_pdfium_render(pdfium, coreExports, false)).toBe(true);
    const engine = new core.Engine();
    const doc = engine.open(readFileSync(fixture));
    expect(engine.pageCount(doc)).toBe(1);
    // US Letter at 72 dpi: one pixel per point.
    const page = engine.renderPage(doc, 0, 72, false);
    const rgba = page.takeRgba();
    expect(rgba.length).toBe(page.width * page.height * 4);
    expect([page.width, page.height]).toEqual([612, 792]);
    let inked = 0;
    for (let i = 0; i < rgba.length; i += 4) {
      if (rgba[i]! < 128 && rgba[i + 1]! < 128 && rgba[i + 2]! < 128) inked++;
    }
    expect(inked).toBeGreaterThan(100);
    page.free();
    expect(engine.close(doc)).toBe(true);
    engine.free();
  });

  it('renders by handle and refuses a closed document, as documentEngine relies on', async () => {
    expect(core.initialize_pdfium_render(pdfium, coreExports, false)).toBe(true);
    const engine = documentEngine(new core.Engine(), coreFunctions, coreEdits);
    const a = await engine.open(readFileSync(fixture));
    const b = await engine.open(readFileSync(fixture));
    expect(a.handle).not.toBe(b.handle);
    await engine.close(b.handle);
    expect((await engine.renderPage(a.handle, 0, 72)).width).toBe(612);
    await expect(engine.renderPage(b.handle, 0, 72)).rejects.toMatchObject({ name: 'DocumentNotFound' });
  });

  it("searches and selects through the core's placement, on the page as drawn", async () => {
    expect(core.initialize_pdfium_render(pdfium, coreExports, false)).toBe(true);
    const engine = documentEngine(new core.Engine(), coreFunctions, coreEdits);
    const { handle, info } = await engine.open(readFileSync(fixture));
    expect(info.textExtractionAllowed).toBe(true);

    // The title line, "Vitela AcroForm interop fixture", is drawn at x 72 pt
    // on a 792 pt tall page, about 40 pt from its top.
    const [hit, ...rest] = await engine.search(handle, 'AcroForm');
    expect(rest).toEqual([]);
    expect(hit!.pageIndex).toBe(0);
    const [left, top, width, height] = hit!.rects;
    expect(hit!.rects).toHaveLength(4);
    expect(left).toBeGreaterThan(72);
    expect(top).toBeGreaterThan(30);
    expect(top).toBeLessThan(60);
    expect(width).toBeGreaterThan(height!);
    expect(await engine.search(handle, 'acroform')).toEqual([]);

    // The middle of that match is on the word, and dragging across it reads it.
    const middle = { x: left! + width! / 2, y: top! + height! / 2 };
    expect((await engine.selectWord(handle, 0, middle)).text).toBe('AcroForm');
    const across = await engine.selectText(handle, 0, { x: left! - 1, y: middle.y }, { x: left! + width!, y: middle.y });
    expect(across.text).toBe('AcroForm');
    await engine.close(handle);
  });

  it('edits pages through the real EditCommand and renders the edit at once', async () => {
    expect(core.initialize_pdfium_render(pdfium, coreExports, false)).toBe(true);
    const engine = documentEngine(new core.Engine(), coreFunctions, coreEdits);
    const { handle, info } = await engine.open(readFileSync(fixture));
    expect(info.documentAssemblyAllowed).toBe(true);
    expect(info.pageIds).toHaveLength(1);

    const turned = await engine.applyEdit(handle, { kind: 'rotate', from: 0, count: 1, degrees: 90 });
    expect(turned).toMatchObject({ canUndo: true, canRedo: false });
    expect(turned.info.pages[0]).toMatchObject({ widthPt: 792, heightPt: 612, rotationDegrees: 90 });
    expect(turned.info.pageIds).toEqual(info.pageIds);
    // No separate refresh: the next render already shows the turn.
    const page = await engine.renderPage(handle, 0, 72);
    expect([page.width, page.height]).toEqual([792, 612]);

    const undone = await engine.undo(handle);
    expect(undone).toMatchObject({ canUndo: false, canRedo: true });
    expect((await engine.renderPage(handle, 0, 72)).width).toBe(612);
    await expect(engine.applyEdit(handle, { kind: 'remove', from: 3, count: 1 })).rejects.toMatchObject({
      name: 'PageIndexOutOfBounds',
    });
    await engine.close(handle);
  });

  it('imports a PDF through the real core as one step, with its own block', async () => {
    expect(core.initialize_pdfium_render(pdfium, coreExports, false)).toBe(true);
    const engine = documentEngine(new core.Engine(), coreFunctions, coreEdits);
    const { handle, info } = await engine.open(readFileSync(fixture));
    expect(info.importRefusal).toBeNull();
    expect(info.blocks).toEqual([{ source: { kind: 'base' }, part: null, start: 0, count: 1 }]);

    const source = await engine.prepareImport(readFileSync(fixture));
    expect(source.pageCount).toBe(1);
    const imported = await engine.importPrepared(handle, [source.id], 0);
    expect(imported.sourceIds).toHaveLength(1);
    expect(imported.info.pageCount).toBe(2);
    expect(imported.info.blocks).toEqual([
      { source: { kind: 'imported', id: imported.sourceIds[0] }, part: null, start: 0, count: 1 },
      { source: { kind: 'base' }, part: null, start: 1, count: 1 },
    ]);
    // Refreshed inside the import: the imported page renders at once.
    expect((await engine.renderPage(handle, 0, 72)).width).toBe(612);

    const undone = await engine.undo(handle);
    expect(undone.info.pageCount).toBe(1);
    await expect(engine.importPrepared(handle, [source.id], 0)).rejects.toMatchObject({
      name: 'UnsupportedOperation',
    });
    await engine.close(handle);
  });

  it('highlights through the real core: no refresh, saved as /Highlight, painted on reopen, taken out by undo', async () => {
    expect(core.initialize_pdfium_render(pdfium, coreExports, false)).toBe(true);
    const engine = documentEngine(new core.Engine(), coreFunctions, coreEdits);
    const { handle, info } = await engine.open(readFileSync(fixture));
    expect(info.annotationEditingAllowed).toBe(true);
    const before = await engine.saveToBytes(handle, false);
    expect(highlights(before)).toBe(0);

    // A band across the title line, on the page as drawn (top-left origin).
    const band = { left: 72, top: 30, width: 200, height: 20 };
    const yellow = { r: 255, g: 255, b: 0 };
    const added = await engine.applyEdit(handle, { kind: 'addMarkup', page: 0, markup: 'highlight', rect: band, color: yellow });
    expect(added).toMatchObject({ canUndo: true, addedAnnotationId: 0 });
    expect(added.annotations).toHaveLength(1);
    const [highlight] = added.annotations;
    expect(highlight).toMatchObject({ kind: 'highlight', id: 0, pageIndex: 0, color: yellow });
    expectBox(highlight!.kind === 'highlight' ? highlight!.rect : null, band);
    // The session draws it: the preview leaves it out, so the page still renders unmarked.
    expect(yellowPixels(await engine.renderPage(handle, 0, 72), band)).toBe(0);

    const saved = await engine.saveToBytes(handle, false);
    expect(highlights(saved)).toBe(1);
    const reopened = await engine.open(saved);
    // Reopened, it is the file's own: pdfium paints it, and no session holds it.
    expect(yellowPixels(await engine.renderPage(reopened.handle, 0, 72), band)).toBeGreaterThan(100);
    expect(await engine.annotations(reopened.handle)).toEqual([]);
    await engine.close(reopened.handle);

    const undone = await engine.undo(handle);
    expect(undone).toMatchObject({ canUndo: false, canRedo: true, annotations: [] });
    expect(highlights(await engine.saveToBytes(handle, false))).toBe(0);
    await engine.close(handle);
  });

  it('keeps a markup box where it was drawn on a page drawn turned, and lists a session note', async () => {
    expect(core.initialize_pdfium_render(pdfium, coreExports, false)).toBe(true);
    const engine = documentEngine(new core.Engine(), coreFunctions, coreEdits);
    const { handle } = await engine.open(readFileSync(fixture));
    await engine.applyEdit(handle, { kind: 'rotate', from: 0, count: 1, degrees: 90 });
    const band = { left: 100, top: 50, width: 30, height: 200 };
    const added = await engine.applyEdit(handle, {
      kind: 'addMarkup',
      page: 0,
      markup: 'underline',
      rect: band,
      color: { r: 0, g: 0, b: 255 },
    });
    const [underline] = added.annotations;
    expectBox(underline!.kind === 'underline' ? underline!.rect : null, band);

    const moved = await engine.applyEdit(handle, { kind: 'moveAnnotation', id: 0, dx: 10, dy: 20 });
    expectBox(moved.annotations[0]!.kind === 'underline' ? moved.annotations[0]!.rect : null, {
      ...band,
      left: band.left + 10,
      top: band.top + 20,
    });

    await engine.applyEdit(handle, { kind: 'addNote', page: 0, rect: { left: 300, top: 300, width: 20, height: 20 }, contents: 'Mine' });
    const comments = await engine.comments(handle);
    expect(comments.at(-1)).toMatchObject({ contents: 'Mine', author: null, date: null, annotationId: 1 });
    await expect(engine.freeTextLayout('日', 100, 40)).rejects.toMatchObject({ name: 'EncodingGap' });
    await engine.close(handle);
  });

  it('fills the four kinds through the real core: painted at once, saved, read back on reopen, taken back by undo', async () => {
    expect(core.initialize_pdfium_render(pdfium, coreExports, false)).toBe(true);
    const engine = documentEngine(new core.Engine(), coreFunctions, coreEdits);
    const { handle, info } = await engine.open(readFileSync(fixture));
    expect(info.formFillAllowed).toBe(true);
    const fields = await engine.formFields(handle);
    // The listbox is a kind the core does not model: not offered.
    expect(fields.map((field) => [field.name, field.kind])).toEqual(
      expect.arrayContaining([
        ['full_name', 'text'],
        ['notes', 'text'],
        ['subscribe', 'checkbox'],
        ['plan', 'radio'],
        ['country', 'dropdown'],
      ]),
    );
    expect(fields).toHaveLength(5);
    const byName = (list: readonly FormField[], name: string) => list.find((field) => field.name === name)!;
    const fullName = byName(fields, 'full_name');
    // reportlab's box, x=72 y=680 240x22 in PDF space, on the page as drawn.
    expectBox(fullName.rect, { left: 72, top: 90, width: 240, height: 22 });
    expect(fullName).toMatchObject({ kind: 'text', value: 'Ada Lovelace', existing: true });
    const plan = byName(fields, 'plan');
    expect(plan.kind === 'radio' && plan.buttons.map((button) => button.exportValue)).toEqual(['basic', 'pro']);
    expect(plan).toMatchObject({ selected: 'pro' });

    const before = inkIn(await engine.renderPage(handle, 0, 72), fullName.rect);
    const filled = await engine.applyEdit(handle, {
      kind: 'setFieldValue',
      fieldId: fullName.id,
      value: { kind: 'text', text: 'José Peña Núñez, Ingeniería' },
    });
    expect(filled).toMatchObject({ canUndo: true, addedAnnotationId: null });
    expect(byName(filled.formFields, 'full_name')).toMatchObject({ value: 'José Peña Núñez, Ingeniería' });
    // Refreshed inside the request: the very next render shows the new value.
    const after = inkIn(await engine.renderPage(handle, 0, 72), fullName.rect);
    expect(after).not.toBe(before);

    await engine.applyEdit(handle, { kind: 'setFieldValue', fieldId: byName(fields, 'subscribe').id, value: { kind: 'checked', checked: false } });
    await engine.applyEdit(handle, { kind: 'setFieldValue', fieldId: plan.id, value: { kind: 'choice', option: 'basic' } });
    await engine.applyEdit(handle, { kind: 'setFieldValue', fieldId: byName(fields, 'country').id, value: { kind: 'choice', option: 'Chile' } });
    await expect(
      engine.applyEdit(handle, { kind: 'setFieldValue', fieldId: byName(fields, 'country').id, value: { kind: 'choice', option: 'Atlantis' } }),
    ).rejects.toMatchObject({ name: 'UnsupportedOperation' });
    await expect(
      engine.applyEdit(handle, { kind: 'setFieldValue', fieldId: 999, value: { kind: 'text', text: 'x' } }),
    ).rejects.toMatchObject({ name: FORM_FIELD_NOT_FOUND });

    const reopened = await engine.open(await engine.saveToBytes(handle, false));
    const saved = await engine.formFields(reopened.handle);
    expect(byName(saved, 'full_name')).toMatchObject({ value: 'José Peña Núñez, Ingeniería' });
    expect(byName(saved, 'subscribe')).toMatchObject({ checked: false });
    expect(byName(saved, 'plan')).toMatchObject({ selected: 'basic' });
    expect(byName(saved, 'country')).toMatchObject({ selected: 'Chile' });
    await engine.close(reopened.handle);

    // Undo the last three: the text fill is the newest step left, and its undo repaints.
    for (let i = 0; i < 3; i++) await engine.undo(handle);
    const undone = await engine.undo(handle);
    expect(byName(undone.formFields, 'full_name')).toMatchObject({ value: 'Ada Lovelace' });
    expect(undone.canUndo).toBe(false);
    expect(inkIn(await engine.renderPage(handle, 0, 72), fullName.rect)).toBe(before);
    await engine.close(handle);
  });

  it('places a dropped stamp on a page drawn a quarter-turn round as pdf-ffi does', async () => {
    expect(core.initialize_pdfium_render(pdfium, coreExports, false)).toBe(true);
    const engine = documentEngine(new core.Engine(), coreFunctions, coreEdits);
    const { handle } = await engine.open(readFileSync(fixture));
    const turned = await engine.applyEdit(handle, { kind: 'rotate', from: 0, count: 1, degrees: 90 });
    const page = turned.info.pages[0]!;
    const drop = { x: 300, y: 200 };
    const placed = await engine.stampPlacement(handle, 0, WIDE_PNG, drop);

    // pdf-ffi's path: the drop point in PDF space, `stamp_placement` there,
    // the box it returns drawn on the page.
    const [ax, ay] = core.pointToPdf(drop.x, drop.y, page.widthPt, page.heightPt, page.rotationDegrees);
    const native = core.stampPlacement(WIDE_PNG, ax!, ay!);
    const [left, top, width, height] = core.placeRects(Float32Array.from(native), page.widthPt, page.heightPt, page.rotationDegrees);
    expectBox(placed, { left: left!, top: top!, width: width!, height: height! });
    // The 2:1 image shows turned, and the drop point is its drawn top-right
    // corner: the image's own top-left, turned with the page.
    expectBox(placed, { left: drop.x - 72, top: drop.y, width: 72, height: 144 });

    // Inserted at that box, the stamp is where the core placed it.
    const added = await engine.applyEdit(handle, { kind: 'addStamp', page: 0, image: WIDE_PNG, rect: placed });
    expectBox(added.annotations[0]!.kind === 'stamp' ? added.annotations[0]!.rect : null, placed);
    await engine.close(handle);
  });

  it('compresses the scan of the compress corpus through the real core: smaller, reopens, history untouched', async () => {
    expect(core.initialize_pdfium_render(pdfium, coreExports, false)).toBe(true);
    const engine = documentEngine(new core.Engine(), coreFunctions, coreEdits);
    expect(await engine.compressPresets()).toEqual(['lossless', 'balanced', 'small']);
    const scan = readFileSync(resolve(here, '../../../../tests/fixtures/compress/scan_200dpi.pdf'));
    const { handle } = await engine.open(new Uint8Array(scan));
    expect(await engine.compressionRefusal(handle)).toBeNull();
    expect(await engine.compressionInvalidatesSignatures(handle)).toBe(false);
    await engine.applyEdit(handle, { kind: 'rotate', from: 0, count: 1, degrees: 90 });

    const compressed = await engine.saveCompressedToBytes(handle, 'balanced', false);
    expect(compressed.reduced).toBe(true);
    expect(compressed.refusals).toEqual([]);
    expect(compressed.afterBytes).toBe(compressed.bytes.length);
    expect(compressed.savedBytes).toBe(compressed.beforeBytes - compressed.afterBytes);
    expect(compressed.bytes.length).toBeLessThan(scan.length);

    // The copy has the pending turn; the session still has it to undo.
    const copy = await engine.open(compressed.bytes);
    expect(copy.info.pages[0]!.rotationDegrees).toBe(90);
    expect((await engine.renderPage(copy.handle, 0, 36)).width).toBeGreaterThan(0);
    const undone = await engine.undo(handle);
    expect(undone.info.pages[0]!.rotationDegrees).toBe(0);
    await expect(engine.saveCompressedToBytes(handle, 'tiny' as never, false)).rejects.toMatchObject({
      name: 'UnsupportedOperation',
    });
    await engine.close(copy.handle);
    await engine.close(handle);
  });

  it('exports pages through the real core: the pending edits drawn, sized by the DPI, named and encoded by the core', async () => {
    expect(core.initialize_pdfium_render(pdfium, coreExports, false)).toBe(true);
    const engine = documentEngine(new core.Engine(), coreFunctions, coreEdits);
    expect(await engine.imageExportFormats()).toEqual(['png', 'jpeg']);
    expect(await engine.parsePageSelection('1', 1)).toEqual([0]);
    await expect(engine.parsePageSelection('2', 1)).rejects.toMatchObject({ name: 'InvalidPageSelection' });
    expect(await engine.pageImageFileNames('report.pdf', [0], 1, 'jpeg')).toEqual(['report-1.jpg']);

    const { handle } = await engine.open(readFileSync(fixture));
    expect(await engine.firstPageTooLargeToExport(handle, [0], 400)).toBeNull();
    expect(await engine.firstPageTooLargeToExport(handle, [0], 600)).toBe(0);
    const plain = await exportFirstPage(engine, handle, 72, 'png');
    expect(pngSize(plain)).toEqual([612, 792]);

    // An unsaved highlight: the preview leaves it out, the export draws it.
    const band = { left: 72, top: 30, width: 200, height: 20 };
    await engine.applyEdit(handle, { kind: 'addMarkup', page: 0, markup: 'highlight', rect: band, color: { r: 255, g: 255, b: 0 } });
    const highlighted = await exportFirstPage(engine, handle, 72, 'png');
    expect(Buffer.from(highlighted).equals(Buffer.from(plain))).toBe(false);

    // An unsaved quarter turn: the image is landscape, and twice as large at 144 DPI.
    await engine.applyEdit(handle, { kind: 'rotate', from: 0, count: 1, degrees: 90 });
    expect(pngSize(await exportFirstPage(engine, handle, 144, 'png'))).toEqual([1584, 1224]);
    const jpeg = await exportFirstPage(engine, handle, 72, 'jpeg');
    expect([...jpeg.subarray(0, 3)]).toEqual([0xff, 0xd8, 0xff]);

    // Not an edit: both steps are still there to undo.
    expect((await engine.undo(handle)).canUndo).toBe(true);
    await engine.close(handle);
  });
});

/** Opens an export of `handle`, encodes its first page and closes the export. */
async function exportFirstPage(
  engine: ReturnType<typeof documentEngine>,
  handle: number,
  dpi: number,
  format: 'png' | 'jpeg',
): Promise<Uint8Array> {
  const exportHandle = await engine.openImageExport(handle);
  try {
    return await engine.exportPageImage(exportHandle, 0, dpi, format);
  } finally {
    expect(await engine.closeImageExport(exportHandle)).toBe(true);
  }
}

/** A PNG's IHDR width and height, after checking its signature. */
function pngSize(bytes: Uint8Array): [number, number] {
  expect([...bytes.subarray(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return [view.getUint32(16), view.getUint32(20)];
}

/** A 2 x 1 pixel PNG: a stamp twice as wide as it is tall. */
const WIDE_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAAB7QOjdAAAADUlEQVR4nGP4z8AARAAI/gH/xp559wAAAABJRU5ErkJggg==',
  'base64',
);

/** How many `/Highlight` annotations the bytes spell out (the core writes them uncompressed). */
function highlights(bytes: Uint8Array): number {
  return Buffer.from(bytes).toString('latin1').match(/\/Subtype\s*\/Highlight\b/g)?.length ?? 0;
}

/** Strongly yellow pixels (blue well below red and green) inside `box`, at 72 dpi. */
function yellowPixels(page: { width: number; rgba: Uint8ClampedArray }, box: { left: number; top: number; width: number; height: number }) {
  let count = 0;
  for (let y = Math.ceil(box.top); y < box.top + box.height; y++) {
    for (let x = Math.ceil(box.left); x < box.left + box.width; x++) {
      const i = (y * page.width + x) * 4;
      const [r, g, b] = [page.rgba[i]!, page.rgba[i + 1]!, page.rgba[i + 2]!];
      if (r > 200 && g > 200 && b < 120) count++;
    }
  }
  return count;
}

/** Dark pixels inside `box`, at 72 dpi: how much text a field shows. */
function inkIn(page: { width: number; rgba: Uint8ClampedArray }, box: { left: number; top: number; width: number; height: number }) {
  let count = 0;
  for (let y = Math.ceil(box.top); y < box.top + box.height; y++) {
    for (let x = Math.ceil(box.left); x < box.left + box.width; x++) {
      const i = (y * page.width + x) * 4;
      if (page.rgba[i]! < 128 && page.rgba[i + 1]! < 128 && page.rgba[i + 2]! < 128) count++;
    }
  }
  return count;
}

/** The box survived the trip through PDF space (the placement is single precision). */
function expectBox(actual: { left: number; top: number; width: number; height: number } | null, expected: typeof actual) {
  expect(actual).not.toBeNull();
  for (const key of ['left', 'top', 'width', 'height'] as const) expect(actual![key]).toBeCloseTo(expected![key], 3);
}
