// The `Engine` contract over the core's document table. The core already
// keeps documents by id and refuses an id it does not hold, so the id is the
// handle: a render always reaches the document the UI asked for, never
// "whichever one is newest". Kept apart from engine.ts (which loads the WASM)
// so it is tested with a fake core.
//
// Page edits refresh the preview inside the same request: the core edits its
// model at once but renders and reads text from a pdfium snapshot that only
// `refreshPreview` brings up to date, and the worker answers one request at a
// time, so no render can ever land between an edit and its refresh. An
// import is an edit like any other here: one step, refreshed the same way.
//
// Only when the core says the step changed the pixels (`previewStale`, the
// core's `shows_in_session_preview`): an annotation edit does not, because
// the preview leaves the session's annotations out and the UI draws them from
// the snapshot every answer carries. So adding a highlight costs no reopen,
// and undoing across a mixed history refreshes exactly on the page steps.
// A form fill does change the pixels (pdfium paints the field), so a fill,
// and its undo and redo, refresh inside the request like a page edit.
//
// A properties change is a step like a page edit; the core counts it as one
// the preview shows, so it refreshes too (a reopen with the same pixels).
//
// The page-content reads (T-258) are not steps either: each parses the page
// the core holds now, pending edits laid on, and places its boxes on the page
// as drawn like every other box here. A text-run edit (T-259) is page
// content: a step the pixels show, refreshed like a watermark; its move
// delta crosses through the page's frame.
//
// A compressed save is not a step at all: it records nothing and refreshes
// nothing, like an ordinary save, and so is a protected copy (its passwords
// go to the core and are kept nowhere). Neither is an image export: it draws from
// its own snapshot of what a save would write, which the core keeps apart
// from the document, so the preview and the history are left alone.

import type {
  Annotation,
  BlockSource,
  CompressPreset,
  DocumentBlock,
  DocumentHandle,
  DocumentInfo,
  ImageExportFormat,
  Edit,
  EditedDocument,
  Engine,
  FormEdit,
  MetadataEdit,
  WatermarkEdit,
  FormField,
  ImportedDocument,
  OpenedDocument,
  PageEdit,
  PagePoint,
  PageSize,
  PreparedImport,
  ProtectionPasswords,
  RenderedPage,
  TextRunEdit,
  ImageEdit,
} from '../core/types.js';
import {
  PageFrame,
  annotationCommand,
  annotations,
  comments,
  freeTextLayout,
  type CoreAnnotation,
  type CoreAnnotationEdits,
  type CoreComment,
  type CoreFreeTextLines,
} from './annotations.js';
import {
  compressPresets,
  compressRefusal,
  compressedDocument,
  type CoreCompressedSave,
  type CoreCompressRefusal,
} from './compress.js';
import { formFieldCommand, formFields, type CoreFormEdits, type CoreFormField } from './forms.js';
import {
  documentProperties,
  metadataCommand,
  type CoreDocumentProperties,
  type CoreMetadataEdits,
} from './metadata.js';
import {
  watermarkCommand,
  watermarkDefaults,
  type CoreWatermarkEdits,
  type CoreWatermarkLook,
} from './watermark.js';
import {
  fontFamilies,
  fontProgram,
  imageCommand,
  pageContent,
  textBlocks,
  textRunCommand,
  textRunLook,
  textRunStyle,
  type CoreFontProgram,
  type CoreImageEdits,
  type CorePageContent,
  type CoreTextBlock,
  type CoreTextRunEdits,
  type CoreTextRunLook,
  type CoreTextRunStyle,
} from './content.js';
import { splitParts, type CoreSplitFunctions } from './extract.js';
import { imageExportFormats, type CoreExportFunctions } from './imageExport.js';
import { PageText, type CoreGeometry, type CoreText, type Owned } from './pageText.js';

/** core/pdf-wasm's `EditCommand` constructors (the commands are wasm objects). */
export interface CoreEdits
  extends CoreAnnotationEdits,
    CoreFormEdits,
    CoreMetadataEdits,
    CoreWatermarkEdits,
    CoreTextRunEdits,
    CoreImageEdits {
  movePages(from: number, count: number, to: number): Owned;
  rotatePages(from: number, count: number, deltaDegrees: number): Owned;
  removePages(index: number, count: number): Owned;
  insertBlankPage(index: number, landscape: boolean): Owned;
}

/** core/pdf-wasm's free functions: placement, and the geometry asked before an annotation exists. */
export interface CoreFunctions extends CoreGeometry, CoreExportFunctions, CoreSplitFunctions {
  freeTextLayout(contents: string, widthPt: number, heightPt: number): CoreFreeTextLines;
  /** `x, y, width, height` in PDF space. */
  stampPlacement(image: Uint8Array, anchorX: number, anchorY: number): Float64Array;
  /** The core's preset names, strongest-preserving first. */
  compressPresets(): string[];
  /** The core's starting watermark looks. */
  watermarkDefaults(): CoreWatermarkLook;
}

/** The slice of core/pdf-wasm's `Engine` this module uses. */
export interface CoreEngine extends CoreText {
  open(bytes: Uint8Array, password?: string): number;
  close(doc: number): boolean;
  pageCount(doc: number): number;
  pageDimensions(doc: number): (PageSize & Owned)[];
  pageIds(doc: number): Uint32Array;
  textExtractionAllowed(doc: number): boolean;
  documentAssemblyAllowed(doc: number): boolean;
  annotationEditingAllowed(doc: number): boolean;
  formFillAllowed(doc: number): boolean;
  saveToBytes(doc: number, invalidateSignatures: boolean): Uint8Array;
  /** `undefined` (wasm-bindgen's `None`) when the document can be compressed. */
  compressionRefusal(doc: number): CoreCompressRefusal | null | undefined;
  compressionInvalidatesSignatures(doc: number): boolean;
  saveCompressedToBytes(doc: number, preset: string, invalidateSignatures: boolean): CoreCompressedSave;
  documentProperties(doc: number): CoreDocumentProperties;
  /** `undefined` (wasm-bindgen's `None`) when the properties can be changed. */
  metadataRefusal(doc: number): string | null | undefined;
  /** The newest page-content step still applied (0: as opened). */
  paintRevision(doc: number): number;
  /** `undefined` (wasm-bindgen's `None`) when a watermark can be stamped. */
  watermarkRefusal(doc: number): string | null | undefined;
  /** `undefined` (wasm-bindgen's `None`) when an editor of the page content can be offered. */
  contentEditingRefusal(doc: number): string | null | undefined;
  readPageContent(doc: number, page: number): CorePageContent;
  readTextBlocks(doc: number, page: number): CoreTextBlock[];
  readParagraphBlocks(doc: number, page: number): CoreTextBlock[];
  /** `undefined` (wasm-bindgen's `None`) when the file cannot say. */
  textRunStyle(doc: number, page: number, runId: number): CoreTextRunStyle | null | undefined;
  /** `[resource, baseFont, …]`. */
  pageFontFamilies(doc: number, page: number): string[];
  /** Bumped by every checkpoint, and by undo or redo across one. */
  contentBaseGeneration(doc: number): number;
  /** `undefined` (wasm-bindgen's `None`) when the file cannot say. */
  textRunLook(doc: number, page: number, runId: number): CoreTextRunLook | null | undefined;
  /** `undefined` (wasm-bindgen's `None`) for a font that embeds none. */
  textRunFontProgram(doc: number, page: number, runId: number): CoreFontProgram | null | undefined;
  /** Throws the core's refusal; returns when the image could be replaced. */
  checkImageReplacement(doc: number, page: number, imageId: number): void;
  /** `undefined` (wasm-bindgen's `None`) when the document can be protected. */
  protectionRefusal(doc: number): string | null | undefined;
  protectionInvalidatesSignatures(doc: number): boolean;
  protectToBytes(doc: number, openPassword: string, permissionsPassword: string, invalidateSignatures: boolean): Uint8Array;
  /** `undefined` (wasm-bindgen's `None`) when pages can be extracted. */
  extractionRefusal(doc: number): string | null | undefined;
  extractSourceIsSigned(doc: number): boolean;
  extractPagesToBytes(doc: number, pages: Uint32Array): Uint8Array;
  /** `undefined` (wasm-bindgen's `None`) when every page fits. */
  firstPageTooLargeToExport(doc: number, pages: Uint32Array, dpi: number): number | null | undefined;
  /** The export's snapshot: its id (its own namespace, not a document id) and page count. */
  openImageExport(doc: number): Owned & { readonly id: number; readonly pageCount: number };
  exportPageImage(exportId: number, pageIndex: number, dpi: number, format: string): Uint8Array;
  closeImageExport(exportId: number): boolean;
  /** `openImageExport`'s snapshot for printing: no permission asked, drawn at 150 DPI at most. */
  openPrintSnapshot(doc: number): Owned & { readonly id: number; readonly pageCount: number };
  /** The id of the annotation the edit added, else `undefined`. */
  applyEdit(doc: number, edit: Owned): number | undefined;
  undo(doc: number): boolean;
  redo(doc: number): boolean;
  canUndo(doc: number): boolean;
  canRedo(doc: number): boolean;
  refreshPreview(doc: number): void;
  /** Whether a step since the last refresh changed what the pixels must show. */
  previewStale(doc: number): boolean;
  annotations(doc: number): CoreAnnotation[];
  comments(doc: number): CoreComment[];
  formFields(doc: number): CoreFormField[];
  prepareImport(bytes: Uint8Array, password?: string): Owned & {
    readonly id: number;
    readonly pageCount: number;
    readonly warnings: string[];
  };
  discardPrepared(id: number): boolean;
  importPrepared(doc: number, ids: Uint32Array, index: number): Owned & {
    readonly pageCount: number;
    readonly sourceIds: Uint32Array;
  };
  /** `undefined` (wasm-bindgen's `None`) when the document refuses no import. */
  importRefusal(doc: number): string | null | undefined;
  documentBlocks(doc: number): (Owned & {
    readonly source: string;
    readonly sourceId: number | undefined;
    readonly part: number | undefined;
    readonly start: number;
    readonly count: number;
  })[];
  renderPage(doc: number, page: number, dpi: number, invert: boolean): Owned & {
    readonly width: number;
    readonly height: number;
    takeRgba(): Uint8Array;
  };
}

export function documentEngine(core: CoreEngine, functions: CoreFunctions, edits: CoreEdits): Engine {
  const text = new PageText(core, functions);
  /** Each open document's page sizes: the placement of its text needs them. */
  const sizes = new Map<DocumentHandle, readonly PageSize[]>();
  const pagesOf = (document: DocumentHandle) => {
    const pages = sizes.get(document);
    // The core's own refusal, for a document this side never opened.
    if (!pages) throw named('DocumentNotFound', `document ${document} is not open`);
    return pages;
  };
  /** Refuses a document this side never opened, with the core's own error. */
  const assertOpen = (document: DocumentHandle): void => void pagesOf(document);
  const pageOf = (document: DocumentHandle, pageIndex: number) => {
    const page = pagesOf(document)[pageIndex];
    if (!page) throw named('PageIndexOutOfBounds', `page ${pageIndex} is out of bounds`);
    return page;
  };

  /** What the document is now; the text placement follows its new page sizes. */
  const describe = (doc: DocumentHandle): DocumentInfo => {
    const info = documentInfo(core, doc);
    sizes.set(doc, info.pages);
    return info;
  };
  /** The frame of a page as the UI last saw it described, or `undefined` past the end. */
  const frameOf = (document: DocumentHandle) => (page: number) => {
    const size = sizes.get(document)?.[page];
    return size && new PageFrame(size, functions);
  };
  /** The session's annotations on the pages as drawn; call after `describe`. */
  const snapshot = (document: DocumentHandle): Annotation[] => annotations(core.annotations(document), frameOf(document));
  /** The document's form fields on the pages as drawn; call after `describe`. */
  const fields = (document: DocumentHandle): FormField[] => formFields(core.formFields(document), frameOf(document));
  /** Rebuilds the preview from the edits; cached characters belong to pages that may have moved or gone. */
  const refresh = (document: DocumentHandle): void => {
    text.forget(document);
    core.refreshPreview(document);
  };
  /**
   * Runs one step of the edit history, then shows it, so the page list the
   * UI is given never disagrees with the pixels it renders.
   *
   * The rule: a request reverts only the step it recorded itself, and only
   * because that step left the preview behind.
   * - Before the step, a preview still behind is brought up to date; if it
   *   cannot be, an edit is refused before it records anything, while an
   *   undo or redo still runs (it may be the way back). (With
   *   the core's exact `previewStale` this does not happen: taking back an
   *   unshown step leaves the preview fresh again.)
   * - So a stale preview after the step is this step's doing. Only then is
   *   it refreshed, and if the refresh fails `revert` takes back that one
   *   step — which returns the history to what the pixels show.
   * - A step that recorded nothing (an undo with nothing to undo) never
   *   reverts, and one the preview does not show (an annotation edit, which
   *   the UI draws from the snapshot) never refreshes, so neither can be
   *   blamed for a refresh failure it did not cause.
   */
  const step = (
    document: DocumentHandle,
    run: () => { recorded: boolean; added?: number },
    revert: () => void,
    { throughStalePreview = false } = {},
  ): EditedDocument => {
    assertOpen(document);
    if (core.previewStale(document)) {
      try {
        refresh(document);
      } catch (error) {
        // Undo and redo still run: stepping through the history may be the
        // very thing that brings the preview back, and refusing them would
        // leave no way out of a preview that cannot catch up.
        if (!throughStalePreview) throw error;
      }
    }
    const { recorded, added } = run();
    if (recorded && core.previewStale(document)) {
      try {
        refresh(document);
      } catch (error) {
        revert();
        throw error;
      }
    }
    const info = describe(document);
    return {
      info,
      canUndo: core.canUndo(document),
      canRedo: core.canRedo(document),
      annotations: snapshot(document),
      addedAnnotationId: added ?? null,
      formFields: fields(document),
      paintRevision: core.paintRevision(document),
      contentBaseGeneration: core.contentBaseGeneration(document),
    };
  };
  /** Steps back or forth through the history (`run` answers whether it moved). */
  const history = (document: DocumentHandle, run: () => boolean, revert: () => void) =>
    step(document, () => ({ recorded: run() }), revert, { throughStalePreview: true });

  return {
    async open(bytes: Uint8Array, password?: string): Promise<OpenedDocument> {
      const doc = core.open(bytes, password);
      try {
        return { handle: doc, info: describe(doc) };
      } catch (error) {
        core.close(doc);
        throw error;
      }
    },
    async renderPage(document: DocumentHandle, pageIndex: number, dpi: number): Promise<RenderedPage> {
      const page = core.renderPage(document, pageIndex, dpi, false);
      try {
        const rgba = page.takeRgba();
        return {
          width: page.width,
          height: page.height,
          rgba: new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.byteLength),
        };
      } finally {
        page.free();
      }
    },
    async close(document: DocumentHandle): Promise<void> {
      text.forget(document);
      sizes.delete(document);
      core.close(document);
    },
    async saveToBytes(document: DocumentHandle, invalidateSignatures: boolean) {
      assertOpen(document);
      return core.saveToBytes(document, invalidateSignatures);
    },
    async compressPresets() {
      return compressPresets(functions.compressPresets());
    },
    async compressionRefusal(document: DocumentHandle) {
      assertOpen(document);
      return compressRefusal(core.compressionRefusal(document));
    },
    async compressionInvalidatesSignatures(document: DocumentHandle) {
      assertOpen(document);
      return core.compressionInvalidatesSignatures(document);
    },
    async saveCompressedToBytes(document: DocumentHandle, preset: CompressPreset, invalidateSignatures: boolean) {
      assertOpen(document);
      return compressedDocument(core.saveCompressedToBytes(document, preset, invalidateSignatures));
    },
    async documentProperties(document: DocumentHandle) {
      assertOpen(document);
      return documentProperties(core.documentProperties(document));
    },
    async metadataRefusal(document: DocumentHandle) {
      assertOpen(document);
      return core.metadataRefusal(document) ?? null;
    },
    async contentEditingRefusal(document: DocumentHandle) {
      assertOpen(document);
      return core.contentEditingRefusal(document) ?? null;
    },
    // The frame is the page as the UI last saw it described, which is the
    // page the core reads now: every step re-describes the document.
    async readPageContent(document: DocumentHandle, pageIndex: number) {
      const frame = new PageFrame(pageOf(document, pageIndex), functions);
      return pageContent(core.readPageContent(document, pageIndex), frame, pageIndex);
    },
    async readTextBlocks(document: DocumentHandle, pageIndex: number) {
      const frame = new PageFrame(pageOf(document, pageIndex), functions);
      return textBlocks(core.readTextBlocks(document, pageIndex), frame);
    },
    async readParagraphBlocks(document: DocumentHandle, pageIndex: number) {
      const frame = new PageFrame(pageOf(document, pageIndex), functions);
      return textBlocks(core.readParagraphBlocks(document, pageIndex), frame);
    },
    async textRunStyle(document: DocumentHandle, pageIndex: number, runId: number) {
      pageOf(document, pageIndex);
      return textRunStyle(core.textRunStyle(document, pageIndex, runId));
    },
    async pageFontFamilies(document: DocumentHandle, pageIndex: number) {
      pageOf(document, pageIndex);
      return fontFamilies(core.pageFontFamilies(document, pageIndex));
    },
    async contentBaseGeneration(document: DocumentHandle) {
      assertOpen(document);
      return core.contentBaseGeneration(document);
    },
    async textRunLook(document: DocumentHandle, pageIndex: number, runId: number) {
      pageOf(document, pageIndex);
      return textRunLook(core.textRunLook(document, pageIndex, runId));
    },
    async textRunFontProgram(document: DocumentHandle, pageIndex: number, runId: number) {
      pageOf(document, pageIndex);
      return fontProgram(core.textRunFontProgram(document, pageIndex, runId));
    },
    async checkImageReplacement(document: DocumentHandle, pageIndex: number, imageId: number) {
      // The core asks its permission before the page: no `pageOf` first.
      assertOpen(document);
      core.checkImageReplacement(document, pageIndex, imageId);
    },
    async watermarkDefaults() {
      return watermarkDefaults(functions.watermarkDefaults());
    },
    async watermarkRefusal(document: DocumentHandle) {
      assertOpen(document);
      return core.watermarkRefusal(document) ?? null;
    },
    async protectionRefusal(document: DocumentHandle) {
      assertOpen(document);
      return core.protectionRefusal(document) ?? null;
    },
    async protectionInvalidatesSignatures(document: DocumentHandle) {
      assertOpen(document);
      return core.protectionInvalidatesSignatures(document);
    },
    async protectToBytes(document: DocumentHandle, passwords: ProtectionPasswords, invalidateSignatures: boolean) {
      assertOpen(document);
      return core.protectToBytes(document, passwords.openPassword, passwords.permissionsPassword, invalidateSignatures);
    },
    async imageExportFormats() {
      return imageExportFormats(functions.imageExportFormats());
    },
    async parsePageSelection(input: string, totalPages: number) {
      return Array.from(functions.parsePageSelection(input, totalPages));
    },
    async pageImageFileNames(documentName: string, pages: readonly number[], totalPages: number, format: ImageExportFormat) {
      return pages.map((page) => functions.pageImageFileName(documentName, page, totalPages, format));
    },
    async firstPageTooLargeToExport(document: DocumentHandle, pages: readonly number[], dpi: number) {
      assertOpen(document);
      return core.firstPageTooLargeToExport(document, Uint32Array.from(pages), dpi) ?? null;
    },
    async openImageExport(document: DocumentHandle) {
      assertOpen(document);
      const opened = core.openImageExport(document);
      try {
        return { handle: opened.id, pageCount: opened.pageCount };
      } finally {
        opened.free();
      }
    },
    // An export outlives nothing on this side: the core refuses an id it does
    // not have (`DocumentNotFound`), whatever document it came from.
    async exportPageImage(exportHandle: number, pageIndex: number, dpi: number, format: ImageExportFormat) {
      return core.exportPageImage(exportHandle, pageIndex, dpi, format);
    },
    async closeImageExport(exportHandle: number) {
      return core.closeImageExport(exportHandle);
    },
    async openPrintSnapshot(document: DocumentHandle) {
      assertOpen(document);
      const opened = core.openPrintSnapshot(document);
      try {
        return { handle: opened.id, pageCount: opened.pageCount };
      } finally {
        opened.free();
      }
    },
    async extractionRefusal(document: DocumentHandle) {
      assertOpen(document);
      return core.extractionRefusal(document) ?? null;
    },
    async extractSourceIsSigned(document: DocumentHandle) {
      assertOpen(document);
      return core.extractSourceIsSigned(document);
    },
    async extractPages(document: DocumentHandle, pages: readonly number[]) {
      assertOpen(document);
      return core.extractPagesToBytes(document, Uint32Array.from(pages));
    },
    async planSplit(cuts: string, totalPages: number, documentName: string) {
      return splitParts(functions.planSplit(cuts, totalPages, documentName));
    },
    async applyEdit(document: DocumentHandle, edit: Edit) {
      return step(
        document,
        () => {
          assertOpen(document);
          const command = isPageEdit(edit)
            ? pageCommand(edits, edit)
            : isFormEdit(edit)
              ? formFieldCommand(edits, edit, () => core.formFillAllowed(document))
              : isMetadataEdit(edit)
                ? metadataCommand(edits, edit)
                : isWatermarkEdit(edit)
                  ? watermarkCommand(edits, edit)
                  : isTextRunEdit(edit)
                    ? textRunCommand(edits, edit, frameOf(document))
                    : isImageEdit(edit)
                      ? imageCommand(edits, edit, frameOf(document))
                      : annotationCommand(
                  edits,
                  edit,
                  frameOf(document),
                  (id) => annotationPage(document, id),
                  () => core.annotationEditingAllowed(document),
                );
          try {
            // A refusal throws, so this recorded the step (or the core's
            // no-op, which cannot leave the preview behind the catch-up).
            return { recorded: true, added: core.applyEdit(document, command) };
          } finally {
            command.free();
          }
        },
        () => void core.undo(document),
      );
    },
    async undo(document: DocumentHandle) {
      return history(document, () => core.undo(document), () => void core.redo(document));
    },
    async redo(document: DocumentHandle) {
      return history(document, () => core.redo(document), () => void core.undo(document));
    },
    async prepareImport(bytes: Uint8Array, password?: string): Promise<PreparedImport> {
      const prepared = core.prepareImport(bytes, password);
      try {
        return { id: prepared.id, pageCount: prepared.pageCount, warnings: [...prepared.warnings] };
      } finally {
        prepared.free();
      }
    },
    async discardImport(id: number) {
      core.discardPrepared(id);
    },
    async importPrepared(document: DocumentHandle, ids: readonly number[], index: number): Promise<ImportedDocument> {
      let sourceIds: number[] = [];
      const edited = step(
        document,
        () => {
          const report = core.importPrepared(document, Uint32Array.from(ids), index);
          try {
            sourceIds = Array.from(report.sourceIds);
          } finally {
            report.free();
          }
          return { recorded: true };
        },
        () => void core.undo(document),
      );
      return { ...edited, sourceIds };
    },
    async search(document: DocumentHandle, query: string) {
      return text.search(document, pagesOf(document), query);
    },
    async selectText(document: DocumentHandle, pageIndex: number, anchor: PagePoint, focus: PagePoint) {
      return text.selectText(document, pageIndex, pageOf(document, pageIndex), anchor, focus);
    },
    async selectWord(document: DocumentHandle, pageIndex: number, point: PagePoint) {
      return text.selectWord(document, pageIndex, pageOf(document, pageIndex), point);
    },
    async annotations(document: DocumentHandle) {
      assertOpen(document);
      return snapshot(document);
    },
    async comments(document: DocumentHandle) {
      assertOpen(document);
      return comments(core.comments(document), frameOf(document));
    },
    async formFields(document: DocumentHandle) {
      assertOpen(document);
      return fields(document);
    },
    async freeTextLayout(contents: string, widthPt: number, heightPt: number) {
      const lines = functions.freeTextLayout(contents, widthPt, heightPt);
      try {
        return freeTextLayout(lines);
      } finally {
        lines.free();
      }
    },
    async stampPlacement(document: DocumentHandle, pageIndex: number, image: Uint8Array, anchor: PagePoint) {
      // The whole placement is the core's (pdf-ffi's `stamp_placement`): the
      // anchor crosses into PDF space and the box the core puts there comes
      // back drawn, so a turned page shows it exactly where the native
      // shells do (the drop point at the image's own top-left, turned).
      const frame = new PageFrame(pageOf(document, pageIndex), functions);
      const [x, y] = frame.pointToPdf(anchor);
      return frame.rectFromPdf(functions.stampPlacement(image, x, y));
    },
  };

  /** Where annotation `id`'s page is now, or `undefined` when the session holds no such annotation. */
  function annotationPage(document: DocumentHandle, id: number): number | undefined {
    let page: number | undefined;
    for (const entry of core.annotations(document)) {
      if (entry.id === id) page = entry.page;
      entry.free();
    }
    return page;
  }
}

function isPageEdit(edit: Edit): edit is PageEdit {
  return edit.kind === 'move' || edit.kind === 'rotate' || edit.kind === 'remove' || edit.kind === 'insertBlank';
}

function isFormEdit(edit: Edit): edit is FormEdit {
  return edit.kind === 'setFieldValue';
}

function isMetadataEdit(edit: Edit): edit is MetadataEdit {
  return edit.kind === 'setDocumentProperties';
}

function isWatermarkEdit(edit: Edit): edit is WatermarkEdit {
  return edit.kind === 'addWatermark';
}

function isTextRunEdit(edit: Edit): edit is TextRunEdit {
  return (
    edit.kind === 'retypeTextRun' ||
    edit.kind === 'moveTextRun' ||
    edit.kind === 'removeTextRun' ||
    edit.kind === 'restyleTextRuns'
  );
}

function isImageEdit(edit: Edit): edit is ImageEdit {
  return (
    edit.kind === 'moveImage' ||
    edit.kind === 'resizeImage' ||
    edit.kind === 'removeImage' ||
    edit.kind === 'replaceImage' ||
    edit.kind === 'insertImage'
  );
}

/** An `Error` named like the core's own refusals, so the UI branches alike. */
function named(name: string, message: string): Error {
  const error = new Error(message);
  error.name = name;
  return error;
}

function pageCommand(edits: CoreEdits, edit: PageEdit): Owned {
  switch (edit.kind) {
    case 'move':
      return edits.movePages(edit.from, edit.count, edit.to);
    case 'rotate':
      return edits.rotatePages(edit.from, edit.count, edit.degrees);
    case 'remove':
      return edits.removePages(edit.from, edit.count);
    case 'insertBlank':
      return edits.insertBlankPage(edit.at, edit.orientation === 'landscape');
  }
}

function documentInfo(core: CoreEngine, doc: number): DocumentInfo {
  return {
    pageCount: core.pageCount(doc),
    pages: pageSizes(core, doc),
    pageIds: Array.from(core.pageIds(doc)),
    textExtractionAllowed: core.textExtractionAllowed(doc),
    documentAssemblyAllowed: core.documentAssemblyAllowed(doc),
    blocks: documentBlocks(core, doc),
    importRefusal: core.importRefusal(doc) ?? null,
    annotationEditingAllowed: core.annotationEditingAllowed(doc),
    formFillAllowed: core.formFillAllowed(doc),
  };
}

/** Copies the core's blocks into plain objects and frees the wasm ones. */
function documentBlocks(core: CoreEngine, doc: number): DocumentBlock[] {
  return core.documentBlocks(doc).map((block) => {
    try {
      return { source: blockSource(block.source, block.sourceId), part: block.part ?? null, start: block.start, count: block.count };
    } finally {
      block.free();
    }
  });
}

function blockSource(kind: string, id: number | undefined): BlockSource {
  if (kind === 'imported' && id !== undefined) return { kind: 'imported', id };
  return kind === 'blank' ? { kind: 'blank' } : { kind: 'base' };
}

/** Copies the core's page sizes into plain objects and frees the wasm ones. */
function pageSizes(core: CoreEngine, doc: number): PageSize[] {
  return core.pageDimensions(doc).map((dimensions) => {
    try {
      const { widthPt, heightPt, rotationDegrees } = dimensions;
      return { widthPt, heightPt, rotationDegrees };
    } finally {
      dimensions.free();
    }
  });
}
