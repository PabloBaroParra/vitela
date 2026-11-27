// The engine behind the worker: pdfium (Emscripten) + core/pdf-wasm
// (wasm-bindgen). This file is the ONLY place that imports the Rust module;
// everything else talks to the `Engine` interface in core/types.ts.

import initEngine, * as core from '../generated/engine/pdf_wasm.js';
import type { Engine } from '../core/types.js';
import { ASSET_FILE_NAMES, type AssetName, type AssetPlan } from './assets.js';
import { documentEngine, type CoreEdits, type CoreFunctions } from './documentEngine.js';
import { fetchAsset } from './fetchAsset.js';
import { verifyAsset } from './integrity.js';
import { instantiatePdfium } from './pdfium.js';

/** Loads both WASM modules, binds pdfium-render to pdfium, starts the core. */
export async function loadEngine(plan: AssetPlan): Promise<Engine> {
  // Fetched here rather than by wasm-bindgen / Emscripten, so every file gets
  // the same deadline and error messages, and is verified before it runs.
  const [engineWasm, pdfiumGlue, pdfiumWasm] = await Promise.all([
    loadAsset(plan, 'engineWasm'),
    loadAsset(plan, 'pdfiumGlue'),
    loadAsset(plan, 'pdfiumWasm'),
  ]);
  const [pdfium, engineExports] = await Promise.all([
    instantiatePdfium(new TextDecoder().decode(pdfiumGlue), new Uint8Array(pdfiumWasm)),
    initEngine({ module_or_path: engineWasm }),
  ]);
  if (!core.initialize_pdfium_render(pdfium, engineExports, false)) {
    throw new Error('initialize_pdfium_render refused the pdfium module');
  }
  return documentEngine(new core.Engine(), coreFunctions, coreEdits);
}

/** The core's free functions, as `documentEngine` takes them. */
export const coreFunctions: CoreFunctions = {
  placeRects: core.placeRects,
  pointToPdf: core.pointToPdf,
  freeTextLayout: core.freeTextLayout,
  stampPlacement: core.stampPlacement,
  compressPresets: core.compressPresets,
  watermarkDefaults: core.watermarkDefaults,
  imageExportFormats: core.imageExportFormats,
  parsePageSelection: core.parsePageSelection,
  pageImageFileName: core.pageImageFileName,
  planSplit: core.planSplit,
};

/** The core's `EditCommand` constructors, as `documentEngine` takes them. */
export const coreEdits: CoreEdits = {
  movePages: (from, count, to) => core.EditCommand.movePages(from, count, to),
  rotatePages: (from, count, degrees) => core.EditCommand.rotatePages(from, count, degrees),
  removePages: (index, count) => core.EditCommand.removePages(index, count),
  insertBlankPage: (index, landscape) => core.EditCommand.insertBlankPage(index, landscape),
  addHighlight: (page, x, y, w, h, rgb) => core.EditCommand.addHighlight(page, x, y, w, h, rgb),
  addUnderline: (page, x, y, w, h, rgb) => core.EditCommand.addUnderline(page, x, y, w, h, rgb),
  addStrikeout: (page, x, y, w, h, rgb) => core.EditCommand.addStrikeout(page, x, y, w, h, rgb),
  addShape: (page, x, y, w, h, rgb) => core.EditCommand.addShape(page, x, y, w, h, rgb),
  addInk: (page, points, rgb) => core.EditCommand.addInk(page, points, rgb),
  addNote: (page, x, y, w, h, contents) => core.EditCommand.addNote(page, x, y, w, h, contents),
  addFreeText: (page, x, y, w, h, contents) => core.EditCommand.addFreeText(page, x, y, w, h, contents),
  addStamp: (page, image, x, y, w, h) => core.EditCommand.addStamp(page, image, x, y, w, h),
  moveAnnotation: (id, dx, dy) => core.EditCommand.moveAnnotation(id, dx, dy),
  resizeAnnotation: (id, x, y, w, h) => core.EditCommand.resizeAnnotation(id, x, y, w, h),
  recolorAnnotation: (id, rgb) => core.EditCommand.recolorAnnotation(id, rgb),
  retextAnnotation: (id, contents) => core.EditCommand.retextAnnotation(id, contents),
  removeAnnotation: (id) => core.EditCommand.removeAnnotation(id),
  setFieldText: (id, text) => core.EditCommand.setFieldText(id, text),
  setFieldChecked: (id, checked) => core.EditCommand.setFieldChecked(id, checked),
  setFieldChoice: (id, option) => core.EditCommand.setFieldChoice(id, option),
  setDocumentProperties: (title, author, subject, keywords, creator, producer) =>
    core.EditCommand.setDocumentProperties(title, author, subject, keywords, creator, producer),
  addTextWatermark: (pages, text, fontSize, opacity, rotation, behind) =>
    core.EditCommand.addTextWatermark(pages, text, fontSize, opacity, rotation, behind),
  addPictureWatermark: (pages, image, scale, opacity, rotation, behind) =>
    core.EditCommand.addPictureWatermark(pages, image, scale, opacity, rotation, behind),
  retypeTextRun: (page, runId, text) => core.EditCommand.retypeTextRun(page, runId, text),
  moveTextRun: (page, runId, dx, dy) => core.EditCommand.moveTextRun(page, runId, dx, dy),
  removeTextRun: (page, runId) => core.EditCommand.removeTextRun(page, runId),
  restyleTextRuns: (page, runIds, bold, italic, underline, strike) =>
    core.EditCommand.restyleTextRuns(page, runIds, bold, italic, underline, strike),
  moveImage: (page, imageId, dx, dy) => core.EditCommand.moveImage(page, imageId, dx, dy),
  resizeImage: (page, imageId, x, y, width, height) => core.EditCommand.resizeImage(page, imageId, x, y, width, height),
  removeImage: (page, imageId) => core.EditCommand.removeImage(page, imageId),
  replaceImage: (page, imageId, bytes) => core.EditCommand.replaceImage(page, imageId, bytes),
  insertImage: (page, x, y, bytes) => core.EditCommand.insertImage(page, x, y, bytes),
  retypeTextBlock: (page, runIds, text) => core.EditCommand.retypeTextBlock(page, runIds, text),
  insertText: (page, x, y, text, size) => core.EditCommand.insertText(page, x, y, text, size),
};

async function loadAsset(plan: AssetPlan, name: AssetName): Promise<ArrayBuffer> {
  const bytes = await fetchAsset(plan.urls[name], plan.timeoutMs);
  if (plan.integrity) {
    const fileName = ASSET_FILE_NAMES[name];
    await verifyAsset(fileName, bytes, plan.integrity[fileName]);
  }
  return bytes;
}
