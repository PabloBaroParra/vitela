// Dispatches one request from the UI to the engine and shapes the reply.
// Pure apart from the engine it is given, so it is tested with a fake one.

import { errorKind, errorMessage } from '../core/errors.js';
import type { EngineResults, RequestEnvelope, WorkerMessage } from '../core/protocol.js';
import type { Engine } from '../core/types.js';

export interface Reply {
  readonly message: WorkerMessage;
  readonly transfer: Transferable[];
}

export async function handleRequest(engine: Engine, { id, request }: RequestEnvelope): Promise<Reply> {
  try {
    switch (request.kind) {
      case 'open':
        return {
          message: { type: 'result', id, value: await engine.open(request.bytes, request.password) },
          transfer: [],
        };
      case 'renderPage': {
        const page = await engine.renderPage(request.document, request.pageIndex, request.dpi);
        return { message: { type: 'result', id, value: page }, transfer: [page.rgba.buffer as ArrayBuffer] };
      }
      case 'close':
        await engine.close(request.document);
        return { message: { type: 'result', id, value: undefined }, transfer: [] };
      case 'save': {
        const bytes = await engine.saveToBytes(request.document, request.invalidateSignatures);
        // A whole PDF: handed over, not copied.
        return { message: { type: 'result', id, value: bytes }, transfer: [bytes.buffer as ArrayBuffer] };
      }
      // An edit's answer is the document's page list: small, copied.
      case 'applyEdit':
        return result(id, await engine.applyEdit(request.document, request.edit));
      case 'undo':
        return result(id, await engine.undo(request.document));
      case 'redo':
        return result(id, await engine.redo(request.document));
      case 'prepareImport':
        return result(id, await engine.prepareImport(request.bytes, request.password));
      case 'discardImport':
        await engine.discardImport(request.id);
        return result(id, undefined);
      case 'importPrepared':
        return result(id, await engine.importPrepared(request.document, request.ids, request.index));
      // Text replies are small (a few rects per line or match): copied, not
      // transferred.
      case 'search':
        return result(id, await engine.search(request.document, request.query));
      case 'selectText': {
        const { document, pageIndex, anchor, focus } = request;
        return result(id, await engine.selectText(document, pageIndex, anchor, focus));
      }
      case 'selectWord':
        return result(id, await engine.selectWord(request.document, request.pageIndex, request.point));
      // Annotation replies are a few numbers per annotation: copied.
      case 'annotations':
        return result(id, await engine.annotations(request.document));
      case 'comments':
        return result(id, await engine.comments(request.document));
      // Form fields too: a name, a box and a value each.
      case 'formFields':
        return result(id, await engine.formFields(request.document));
      case 'freeTextLayout':
        return result(id, await engine.freeTextLayout(request.contents, request.widthPt, request.heightPt));
      case 'stampPlacement': {
        const { document, pageIndex, image, anchor } = request;
        return result(id, await engine.stampPlacement(document, pageIndex, image, anchor));
      }
      // The compress questions are a list of names, a refusal and a yes/no: copied.
      case 'compressPresets':
        return result(id, await engine.compressPresets());
      case 'compressionRefusal':
        return result(id, await engine.compressionRefusal(request.document));
      case 'compressionInvalidatesSignatures':
        return result(id, await engine.compressionInvalidatesSignatures(request.document));
      case 'saveCompressed': {
        const { document, preset, invalidateSignatures } = request;
        const compressed = await engine.saveCompressedToBytes(document, preset, invalidateSignatures);
        // The report is small; the PDF in it is handed over, not copied.
        return {
          message: { type: 'result', id, value: compressed },
          transfer: [compressed.bytes.buffer as ArrayBuffer],
        };
      }
      // The image-export questions are lists of numbers and names: copied.
      case 'imageExportFormats':
        return result(id, await engine.imageExportFormats());
      case 'parsePageSelection':
        return result(id, await engine.parsePageSelection(request.input, request.totalPages));
      case 'pageImageFileNames': {
        const { documentName, pages, totalPages, format } = request;
        return result(id, await engine.pageImageFileNames(documentName, pages, totalPages, format));
      }
      case 'firstPageTooLargeToExport':
        return result(id, await engine.firstPageTooLargeToExport(request.document, request.pages, request.dpi));
      case 'openImageExport':
        return result(id, await engine.openImageExport(request.document));
      case 'exportPageImage': {
        const { pageIndex, dpi, format } = request;
        const image = await engine.exportPageImage(request.export, pageIndex, dpi, format);
        // A whole image file: handed over, not copied.
        return { message: { type: 'result', id, value: image }, transfer: [image.buffer as ArrayBuffer] };
      }
      case 'closeImageExport':
        return result(id, await engine.closeImageExport(request.export));
    }
  } catch (error) {
    // The core names every failure kind (`WrongPassword`, ...): the UI
    // branches on that name, so it must cross the worker boundary too.
    return { message: { type: 'error', id, name: errorKind(error) ?? 'Error', message: errorMessage(error) }, transfer: [] };
  }
}

function result(id: number, value: EngineResults[keyof EngineResults]): Reply {
  return { message: { type: 'result', id, value }, transfer: [] };
}
